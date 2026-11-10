//! Profiles: a player's own directory of quest progress, later saves and
//! settings. openregent's own format, not the original's: HD keeps one
//! `questdata_majx.sav` for the install and asks a name it keys nothing
//! by (`docs/formats/panels.md`, the quest map).
//!
//! ```text
//!  $XDG_DATA_HOME/openregent/
//!    profiles.toml              version = 1, last = "<dir>"
//!    iroh.toml                  version = 1, secret: the iroh key (`net::iroh`)
//!    profiles/<dir>/
//!      profile.toml             version = 1, name = "Your Majesty", nick = "Ann"
//!      lore.toml                seen, owned and credited kills by unit id
//!      progress.toml            version = 1, last_won = "QU01"
//!                               [seen] advisor, expert_hint, ending
//!                               [quests.QU01] victories = 2, best_ms = 900000
//!                               [quests."{31305551-0054-0032-…}"]  a mod's quest
//! ```
//!
//! `<dir>` is the name made safe ([`folder`]): letters, digits, space,
//! `-` and `_` kept, anything else `_`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use regent_core::data::fourcc::FourCc;
use regent_core::data::guid::Guid;
use regent_core::view::menu::quest_map::{Progress, Record, Seen};
use serde::{Deserialize, Serialize};

use super::store::{self, VERSION, checked, read, write};

/// The profiles' directory in [`crate::files::env::OWN`].
const PROFILES: &str = "profiles";

/// What profile was used last, in [`crate::files::env::OWN`].
const LAST: &str = "profiles.toml";

/// A profile's name, in its directory.
const IDENTITY: &str = "profile.toml";

/// A profile's quest progress, in its directory.
const PROGRESS: &str = "progress.toml";

/// Discovery and credited kills, kept outside game saves.
const LORE: &str = "lore.toml";

/// A name's most characters.
pub const MAX_NAME: usize = 32;

/// The name offered for a new profile where `GMTX` is not at hand: the
/// original's prompt's default, `regent_core::view::menu::NAME_TEXT`.
pub const DEFAULT_NAME: &str = "Your Majesty";

/// Why a name will not do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    Empty,
    TooLong,
    /// Nothing but stand-ins once made safe, as `../`.
    Unsafe,
    /// Another profile has the directory, in any case.
    Taken(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::Empty => write!(f, "a profile needs a name"),
            Refused::TooLong => write!(f, "a name of at most {MAX_NAME} characters"),
            Refused::Unsafe => write!(f, "a name needs a letter or digit"),
            Refused::Taken(name) => write!(f, "{name} already has that name"),
        }
    }
}

/// `name`'s directory: trimmed, each character but a letter, digit, space,
/// `-` or `_` made `_`.
///
/// ```text
///  "Your Majesty"  ->  "Your Majesty"
///  "a/b\c"         ->  "a_b_c"
///  ".."            ->  refused
/// ```
///
/// # Errors
///
/// An empty or too long name, or one of nothing but stand-ins.
pub fn folder(name: &str) -> Result<String, Refused> {
    if name.trim().chars().count() > MAX_NAME {
        Err(Refused::TooLong)
    } else {
        safe(name)
    }
}

/// [`folder`] of a name of any length.
///
/// # Errors
///
/// An empty name, or one of nothing but stand-ins.
pub fn safe(name: &str) -> Result<String, Refused> {
    let name = name.trim();

    if name.is_empty() {
        Err(Refused::Empty)
    } else {
        store::stem(name).ok_or(Refused::Unsafe)
    }
}

/// A profile as listed: its directory and name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub dir: String,
    pub name: String,
}

/// The profiles under one data directory.
#[derive(Clone, Debug)]
pub struct Profiles {
    /// [`crate::files::env::OWN`]: [`LAST`] and [`PROFILES`] in it.
    base: PathBuf,
}

/// `profile.toml`.
#[derive(Deserialize, Serialize)]
#[serde(default)]
struct Identity {
    version: u32,
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    nick: String,
}

impl Default for Identity {
    fn default() -> Self {
        Self {
            version: VERSION,
            name: DEFAULT_NAME.to_owned(),
            nick: String::new(),
        }
    }
}

/// `progress.toml`: the last won quest, the seen-once flags, each
/// quest's record by code.
#[derive(Deserialize, Serialize)]
#[serde(default)]
struct ProgressFile {
    version: u32,
    last_won: Option<String>,
    seen: SeenFile,
    quests: BTreeMap<String, Entry>,
}

impl Default for ProgressFile {
    fn default() -> Self {
        Self {
            version: VERSION,
            last_won: None,
            seen: SeenFile::default(),
            quests: BTreeMap::new(),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(default)]
struct LoreFile {
    version: u32,
    entries: BTreeMap<String, LoreMark>,
}

impl Default for LoreFile {
    fn default() -> Self {
        Self {
            version: VERSION,
            entries: BTreeMap::new(),
        }
    }
}

/// One descriptor's permanent discovery.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct LoreMark {
    pub seen: bool,
    pub owned: bool,
    pub kills: u32,
}

/// Units and buildings known by this profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoreProgress(pub BTreeMap<FourCc, LoreMark>);

impl LoreProgress {
    /// Mark an encounter once.
    pub fn see(&mut self, code: FourCc) -> bool {
        let mark = self.0.entry(code).or_default();
        let changed = !mark.seen;
        mark.seen = true;
        changed
    }

    /// Ownership reveals every detail permanently.
    pub fn own(&mut self, code: FourCc) -> bool {
        let mark = self.0.entry(code).or_default();
        let changed = !mark.owned;
        mark.seen = true;
        mark.owned = true;
        changed
    }

    /// Count a credited kill and reveal its type.
    pub fn kill(&mut self, code: FourCc) {
        let mark = self.0.entry(code).or_default();
        mark.seen = true;
        mark.kills = mark.kills.saturating_add(1);
    }

    #[must_use]
    pub fn mark(&self, code: FourCc) -> LoreMark {
        self.0.get(&code).copied().unwrap_or_default()
    }
}

/// The seen-once flags, as written (`quest_map::Seen`).
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools, reason = "one flag a seen-once byte")]
struct SeenFile {
    /// The map advisor's first voice was played.
    advisor: bool,
    /// The expert hint was shown.
    expert_hint: bool,
    /// The expansion's ending was asked.
    ending: bool,
}

/// One quest's record, as written.
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct Entry {
    victories: u32,
    best_ms: u32,
}

/// `profiles.toml`.
#[derive(Deserialize, Serialize)]
#[serde(default)]
struct Last {
    version: u32,
    last: String,
}

impl Default for Last {
    fn default() -> Self {
        Self {
            version: VERSION,
            last: String::new(),
        }
    }
}

impl Profiles {
    /// Under `base`, the directory holding [`LAST`] and [`PROFILES`].
    #[must_use]
    pub fn at(base: &Path) -> Profiles {
        Profiles {
            base: base.to_path_buf(),
        }
    }

    /// Under the XDG data directory, as the log; nothing made yet.
    #[must_use]
    pub fn default_place() -> Option<Profiles> {
        dirs::data_dir().map(|d| Profiles::at(&d.join(crate::files::env::OWN)))
    }

    fn root(&self) -> PathBuf {
        self.base.join(PROFILES)
    }

    /// Every profile with a readable name, by name.
    #[must_use]
    pub fn list(&self) -> Vec<Listed> {
        let mut listed: Vec<Listed> = std::fs::read_dir(self.root())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let dir = entry.file_name().into_string().ok()?;
                let identity: Identity = read(&entry.path().join(IDENTITY)).ok()?;

                Some(Listed {
                    dir,
                    name: identity.name,
                })
            })
            .collect();

        listed.sort_by_key(|l| l.name.to_lowercase());
        listed
    }

    /// A new profile named `name`, its directory made.
    ///
    /// # Errors
    ///
    /// A name [`folder`] refuses, one another profile's directory has, or
    /// a directory that will not be made.
    pub fn create(&self, name: &str) -> Result<Profile, String> {
        let dir = folder(name).map_err(|r| r.to_string())?;
        let taken = self
            .list()
            .into_iter()
            .find(|l| l.dir.to_lowercase() == dir.to_lowercase());

        if let Some(other) = taken {
            Err(Refused::Taken(other.name).to_string())?;
        }

        let path = self.root().join(&dir);
        let identity = Identity {
            version: VERSION,
            name: name.trim().to_owned(),
            nick: String::new(),
        };

        store::make_dir(&path)?;
        write(&path.join(IDENTITY), &identity)?;

        let profile = Profile {
            name: identity.name,
            nick: identity.nick,
            dir: path,
            progress: Progress::default(),
            lore: LoreProgress::default(),
        };

        profile.save()?;
        Ok(profile)
    }

    /// The profile in directory `dir`, its progress read; none yet is
    /// none won.
    ///
    /// # Errors
    ///
    /// A file that will not read, or of a later version.
    pub fn open(&self, dir: &str) -> Result<Profile, String> {
        let path = self.root().join(dir);
        let identity_path = path.join(IDENTITY);
        let identity: Identity = read(&identity_path)?;

        checked(identity.version, &identity_path)?;

        let progress_path = path.join(PROGRESS);
        let progress = if progress_path.exists() {
            progress(&read(&progress_path)?, &progress_path)?
        } else {
            Progress::default()
        };
        let lore_path = path.join(LORE);
        let lore = if lore_path.exists() {
            let file: LoreFile = read(&lore_path)?;
            checked(file.version, &lore_path)?;
            let entries = file
                .entries
                .into_iter()
                .map(|(code, mark)| {
                    FourCc::try_from(code.as_bytes())
                        .map(|code| (code, mark))
                        .map_err(|_| format!("{}: {code} is no unit code", lore_path.display()))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;

            LoreProgress(entries)
        } else {
            LoreProgress::default()
        };

        Ok(Profile {
            name: identity.name,
            nick: identity.nick,
            dir: path,
            progress,
            lore,
        })
    }

    /// The last used profile, read; `None` with none, or one that will
    /// not read.
    #[must_use]
    pub fn opened_last(&self) -> Option<Profile> {
        self.open(&self.last()?).ok()
    }

    /// The last used profile, else the first listed that reads.
    #[must_use]
    pub fn found(&self) -> Option<Profile> {
        self.opened_last()
            .or_else(|| self.list().iter().find_map(|l| self.open(&l.dir).ok()))
    }

    /// The profile a game plays as, asked nothing: the last used, else
    /// the first listed that reads, else a new one named `name`. It is
    /// remembered as the last used.
    ///
    /// # Errors
    ///
    /// A profile that will not be made, or a file that will not be
    /// written.
    pub fn current(&self, name: &str) -> Result<Profile, String> {
        let profile = match self.found() {
            Some(profile) => profile,
            None => self.create(name)?,
        };

        self.remember(&profile)?;
        Ok(profile)
    }

    /// Delete the profile in directory `dir`: its progress and saves.
    ///
    /// # Errors
    ///
    /// A `dir` that is no single directory name, or one that will not
    /// be removed.
    pub fn delete(&self, dir: &str) -> Result<(), String> {
        let path = self.root().join(dir);

        if Path::new(dir).file_name() == Some(std::ffi::OsStr::new(dir)) {
            std::fs::remove_dir_all(&path).map_err(|e| format!("{}: {e}", path.display()))
        } else {
            Err(format!("{dir}: no profile directory"))
        }
    }

    /// The directory of the profile used last, if any.
    #[must_use]
    pub fn last(&self) -> Option<String> {
        let path = self.base.join(LAST);
        let last: Last = read(&path).ok()?;

        checked(last.version, &path)
            .ok()
            .and_then(|()| (!last.last.is_empty()).then_some(last.last))
    }

    /// Remember `profile` as the last used.
    ///
    /// # Errors
    ///
    /// A file that will not be written.
    pub fn remember(&self, profile: &Profile) -> Result<(), String> {
        let dir = profile
            .dir
            .file_name()
            .and_then(|d| d.to_str())
            .ok_or_else(|| format!("{}: no directory name", profile.dir.display()))?;

        write(
            &self.base.join(LAST),
            &Last {
                version: VERSION,
                last: dir.to_owned(),
            },
        )
    }
}

/// `file`'s progress; a name neither four bytes nor a GUID is an error.
fn progress(file: &ProgressFile, path: &Path) -> Result<Progress, String> {
    checked(file.version, path)?;

    let named = |name: &str| match Guid::parse(name) {
        Some(id) => Ok(Named::Id(id)),
        None => FourCc::try_from(name.as_bytes())
            .map(Named::Code)
            .map_err(|_| format!("{}: {name} is no quest code", path.display())),
    };
    let last_won = file.last_won.as_deref().map(named).transpose()?;
    let records = file.quests.iter().map(|(name, e)| {
        let quest = named(name)?;
        let record = Record {
            victories: e.victories,
            best_ms: e.best_ms,
        };

        Ok((quest, record))
    });

    records.collect::<Result<Vec<_>, String>>().map(|records| {
        let named = records.iter().map(|(quest, _)| quest).chain(&last_won);
        let ids: Vec<Guid> = named
            .filter_map(|n| match n {
                Named::Id(id) => Some(*id),
                Named::Code(_) => None,
            })
            .collect();
        let mut progress = Progress::of(records.iter().map(|(q, r)| (q.code(), *r)));

        // A mod's quest's id kept, so it is named by it again.
        for id in ids {
            progress.known(id);
        }

        progress.last_won = last_won.as_ref().map(Named::code);
        progress.seen = Seen {
            advisor: file.seen.advisor,
            expert_hint: file.seen.expert_hint,
            ending: file.seen.ending,
        };
        progress
    })
}

/// A quest as `progress.toml` names it: by code, or a mod's quest by
/// its id, whose code is a CRC of it (`quest::key_of`).
enum Named {
    Code(FourCc),
    Id(Guid),
}

impl Named {
    /// The quest's code.
    fn code(&self) -> FourCc {
        match self {
            Named::Code(code) => *code,
            Named::Id(id) => regent_core::content::quest::key_of(*id),
        }
    }
}

/// One player's profile, open.
#[derive(Clone, Debug)]
pub struct Profile {
    pub name: String,
    /// Empty uses the profile name.
    nick: String,
    /// Its directory.
    pub dir: PathBuf,
    pub progress: Progress,
    pub lore: LoreProgress,
}

impl Profile {
    /// The multiplayer nick, or this profile's name.
    #[must_use]
    pub fn nick(&self) -> &str {
        let nick = self.nick.trim();

        if nick.is_empty() { &self.name } else { nick }
    }

    /// Write this profile's multiplayer nick. Empty uses its name.
    ///
    /// # Errors
    ///
    /// A file that will not be written.
    pub fn save_nick(&mut self, nick: &str) -> Result<(), String> {
        let identity = Identity {
            version: VERSION,
            name: self.name.clone(),
            nick: nick.trim().to_owned(),
        };

        write(&self.dir.join(IDENTITY), &identity)?;
        self.nick = identity.nick;
        Ok(())
    }

    /// Write its progress.
    ///
    /// # Errors
    ///
    /// A file that will not be written.
    pub fn save(&self) -> Result<(), String> {
        let quests = self
            .progress
            .records()
            .map(|(quest, r)| {
                let entry = Entry {
                    victories: r.victories,
                    best_ms: r.best_ms,
                };

                (self.named(quest), entry)
            })
            .collect();
        let seen = self.progress.seen;
        let file = ProgressFile {
            version: VERSION,
            last_won: self.progress.last_won.map(|q| self.named(q)),
            seen: SeenFile {
                advisor: seen.advisor,
                expert_hint: seen.expert_hint,
                ending: seen.ending,
            },
            quests,
        };

        write(&self.dir.join(PROGRESS), &file)
    }

    /// Write lore progress atomically.
    pub fn save_lore(&self) -> Result<(), String> {
        let entries = self
            .lore
            .0
            .iter()
            .map(|(code, mark)| (code.to_string(), *mark))
            .collect();

        write(
            &self.dir.join(LORE),
            &LoreFile {
                version: VERSION,
                entries,
            },
        )
    }

    /// How `progress.toml` names quest `code`: by its id where it is a
    /// CRC of one, a mod's quest's, else by itself.
    fn named(&self, code: FourCc) -> String {
        self.progress
            .id(code)
            .map_or_else(|| code.to_string(), |id| id.to_string())
    }
}

/// The profile games play as, and whether this game's victory is
/// recorded. None only where no profile could be read or made: then
/// nothing is recorded.
#[derive(Resource, Default)]
pub struct Player {
    pub profile: Option<Profile>,
    /// This game's end is recorded; cleared as a game starts.
    pub recorded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch folder of its own.
    fn scratch(name: &str) -> crate::testdata::Scratch {
        crate::testdata::scratch(&format!("profiles-{name}"))
    }

    #[test]
    fn a_name_becomes_a_safe_directory() {
        assert_eq!(folder("Your Majesty"), Ok("Your Majesty".to_owned()));
        assert_eq!(folder("  Kåre  "), Ok("Kåre".to_owned()));
        assert_eq!(folder("a/b\\c:d"), Ok("a_b_c_d".to_owned()));
        assert_eq!(folder("../x"), Ok("___x".to_owned()));
        assert_eq!(folder(".."), Err(Refused::Unsafe));
        assert_eq!(folder("/"), Err(Refused::Unsafe));
        assert_eq!(folder("   "), Err(Refused::Empty));
        assert_eq!(folder(&"x".repeat(MAX_NAME + 1)), Err(Refused::TooLong));
    }

    #[test]
    fn a_profiles_progress_comes_back_as_it_was_written() {
        let dir = scratch("round_trip");
        let profiles = Profiles::at(&dir);
        let mut made = profiles.create("Your Majesty").expect("made");

        made.progress.win(FourCc(*b"QU01"), 900_000);
        made.progress.win(FourCc(*b"QU01"), 600_000);
        made.progress.win(FourCc(*b"VAMP"), 1_200_000);
        made.progress.open_map(&[]);
        made.progress.seen.ending = true;
        made.progress.hinted = false;
        made.save().expect("saved");
        profiles.remember(&made).expect("remembered");

        let opened = profiles.open("Your Majesty").expect("opened");

        assert_eq!(opened.name, "Your Majesty");
        assert_eq!(opened.progress, made.progress);
        assert_eq!(opened.progress.last_won, Some(FourCc(*b"VAMP")));
        assert!(opened.progress.seen.advisor);
        assert!(opened.progress.seen.ending);
        assert_eq!(profiles.last().as_deref(), Some("Your Majesty"));
        assert_eq!(
            profiles.list(),
            [Listed {
                dir: "Your Majesty".to_owned(),
                name: "Your Majesty".to_owned()
            }]
        );
    }

    #[test]
    fn a_multiplayer_nick_stays_with_its_profile() {
        let dir = scratch("profile_nicks");
        let profiles = Profiles::at(&dir);
        let mut ann = profiles.create("Ann").expect("made");
        let bob = profiles.create("Bob").expect("made");

        assert_eq!(ann.nick(), "Ann");
        assert_eq!(bob.nick(), "Bob");

        ann.save_nick("Captain").expect("saved");

        let identity = std::fs::read_to_string(ann.dir.join(IDENTITY)).expect("identity");
        assert!(identity.contains("nick = \"Captain\""));
        assert_eq!(profiles.open("Ann").expect("opened").nick(), "Captain");
        assert_eq!(profiles.open("Bob").expect("opened").nick(), "Bob");

        ann.save_nick("").expect("cleared");

        assert_eq!(profiles.open("Ann").expect("opened").nick(), "Ann");
    }

    #[test]
    fn lore_progress_survives_profile_reopen() {
        let dir = scratch("lore_round_trip");
        let profiles = Profiles::at(&dir);
        let mut made = profiles.create("P").expect("made");
        let minotaur = FourCc(*b"BVM1");
        let palace = FourCc(*b"ABJ1");

        assert!(made.lore.see(minotaur));
        assert!(!made.lore.see(minotaur));
        made.lore.kill(minotaur);
        made.lore.own(palace);
        made.save_lore().expect("saved");

        let opened = profiles.open("P").expect("opened");
        assert_eq!(opened.lore, made.lore);
        assert_eq!(opened.lore.mark(minotaur).kills, 1);
        assert!(opened.lore.mark(palace).owned);
    }

    #[test]
    fn missing_profile_keys_take_defaults() {
        let base = scratch("missing_keys");
        let profiles = Profiles::at(&base);
        let made = profiles.create("P").expect("made");

        std::fs::write(made.dir.join(IDENTITY), "name = \"P\"\n").expect("identity");
        std::fs::write(
            made.dir.join(PROGRESS),
            "[seen]\nadvisor = true\n[quests.QU01]\nvictories = 2\n",
        )
        .expect("progress");
        std::fs::write(made.dir.join(LORE), "[entries.BVM1]\nseen = true\n").expect("lore");
        std::fs::write(base.join(LAST), "last = \"P\"\n").expect("last");

        let opened = profiles.open("P").expect("opened");

        assert_eq!(profiles.last().as_deref(), Some("P"));
        assert_eq!(opened.name, "P");
        assert_eq!(opened.nick(), "P");
        assert_eq!(opened.progress.last_won, None);
        assert!(opened.progress.seen.advisor);
        assert!(!opened.progress.seen.expert_hint);
        assert_eq!(opened.progress.victories(FourCc(*b"QU01")), 2);
        assert_eq!(
            opened.lore.mark(FourCc(*b"BVM1")),
            LoreMark {
                seen: true,
                ..LoreMark::default()
            }
        );
    }

    #[test]
    fn empty_profile_files_take_defaults() {
        let base = scratch("empty_keys");
        let profiles = Profiles::at(&base);
        let made = profiles.create("P").expect("made");

        for file in [IDENTITY, PROGRESS, LORE] {
            std::fs::write(made.dir.join(file), "").expect("empty file");
        }
        std::fs::write(base.join(LAST), "").expect("empty last");

        let opened = profiles.open("P").expect("opened");

        assert_eq!(opened.name, DEFAULT_NAME);
        assert_eq!(opened.progress, Progress::default());
        assert_eq!(opened.lore, LoreProgress::default());
        assert_eq!(profiles.last(), None);
    }

    #[test]
    fn a_name_another_profile_holds_in_any_case_is_refused() {
        let dir = scratch("taken");
        let profiles = Profiles::at(&dir);

        profiles.create("Kåre").expect("made");

        let again = profiles.create(" kÅre ").map(|p| p.name);

        assert_eq!(again, Err(Refused::Taken("Kåre".to_owned()).to_string()));
        assert!(profiles.create("a/b").is_ok());
        assert!(profiles.create("a_b").is_err(), "the same directory");
    }

    #[test]
    fn a_file_of_another_version_is_refused_and_kept() {
        let base = scratch("later");
        let profiles = Profiles::at(&base);
        let made = profiles.create("P").expect("made");
        let path = made.dir.join(PROGRESS);
        let file = |version| {
            format!(
                "version = {version}\n\n[seen]\nadvisor = false\nexpert_hint = false\nending = false\n\n[quests]\n"
            )
        };

        for other in [VERSION - 1, VERSION + 1] {
            std::fs::write(&path, file(other)).expect("written");

            assert!(
                profiles
                    .open("P")
                    .is_err_and(|e| e.contains("this openregent reads"))
            );
            assert_eq!(std::fs::read_to_string(&path).ok(), Some(file(other)));
        }
    }

    #[test]
    fn the_current_profile_is_the_last_else_the_first_else_a_new_one() {
        let dir = scratch("current");
        let profiles = Profiles::at(&dir);
        let made = profiles.current(DEFAULT_NAME).expect("made");

        assert_eq!(made.name, DEFAULT_NAME);
        assert_eq!(profiles.last().as_deref(), Some(DEFAULT_NAME));

        let other = profiles.create("Kåre").expect("made");

        profiles.remember(&other).expect("remembered");
        assert_eq!(
            profiles.current(DEFAULT_NAME).map(|p| p.name),
            Ok("Kåre".to_owned())
        );

        profiles.delete("Kåre").expect("deleted");
        assert_eq!(profiles.list().len(), 1);
        assert_eq!(
            profiles.current("New").map(|p| p.name),
            Ok(DEFAULT_NAME.to_owned()),
            "the first listed"
        );
        assert!(profiles.delete("../x").is_err());
    }

    #[test]
    fn a_profile_with_no_progress_file_has_won_nothing() {
        let dir = scratch("bare");
        let profiles = Profiles::at(&dir);
        let made = profiles.create("P").expect("made");

        std::fs::remove_file(made.dir.join(PROGRESS)).expect("removed");

        assert_eq!(
            profiles.open("P").expect("opened").progress,
            Progress::default()
        );
        assert!(profiles.last().is_none());
    }

    /// A mod's quest's victory is kept by its id, its code a CRC
    /// (Majesty Expanded's Barren Waste, not `QU01`), and comes back so;
    /// the last won too.
    #[test]
    fn a_mod_quests_progress_is_named_by_its_id() {
        const COPY: &str = "{31305551-0054-0032-0000-000000000000}";

        let dir = scratch("mod_quest");
        let profiles = Profiles::at(&dir);
        let mut made = profiles.create("P").expect("made");
        let copy = Guid::parse(COPY).expect("a GUID");

        made.progress.win(FourCc(*b"QU01"), 900_000);
        made.progress.win_quest(copy, 600_000);
        made.save().expect("saved");

        let text = std::fs::read_to_string(made.dir.join(PROGRESS)).expect("read");
        let opened = profiles.open("P").expect("opened");
        let code = regent_core::content::quest::key_of(copy);

        assert!(text.contains(&format!("[quests.\"{COPY}\"]")), "{text}");
        assert!(text.contains(&format!("last_won = \"{COPY}\"")), "{text}");
        assert_eq!(opened.progress, made.progress);
        assert_eq!(opened.progress.victories(code), 1);
        assert_eq!(opened.progress.victories(FourCc(*b"QU01")), 1);
        assert_eq!(opened.progress.last_won, Some(code));
    }
}
