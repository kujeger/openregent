//! Making a game: the one path the command line, the menus, a saved game
//! and the tests take.
//!
//! ```text
//!  setup (options, play.input), its quest ──> Plan: dataset, needs, the scene it starts on
//!  Plan + a catalogue (a corpus: the shelf's, else opened) ──> Plan::begin
//!    founded   the world before its game, and a custom game's setup
//!    Sim       resumed (a save of ours), else set up
//!    Session   quest, goals, setup, needs: what is shown and saved of it
//!  Maker: a menu's or the command line's Source ──> Made, recorded or replayed
//! ```
//!
//! [`Plan::begin`] reads files and makes the game, and touches no Bevy
//! world: the menus run [`Maker::make`] on a thread (`menu::begin`).

use regent_core::sim::freestyle;
use std::path::{Path, PathBuf};

use bevy::log::{info, warn};
use bevy::prelude::Resource;
use regent_core::content::dataset::{self, Bytecode, Datasets};
use regent_core::content::mods::Mod;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::guid::Guid;
use regent_core::gpl::load::Program;
use regent_core::gpl::startup;
use regent_core::gpl::vm::Config;
use regent_core::sim::mapgen::custom::Art;
use regent_core::sim::scene::{Scene, Staging};
use regent_core::sim::world::cheat::Cheats;
use regent_core::sim::world::{World, offer::Menu};
use regent_draw::data::{Corpus, Sources};

use crate::draw::shelf::Shelf;
use crate::files::compiled::{Builds, Built};
use crate::files::env::Install;
use crate::game::load::Offered;
use crate::game::record::{self, Recorder};
use crate::game::sim::{Resumed, Session, Sim};
use crate::menu::freestyle::Custom;
use crate::save::{self, Needs};
use crate::setup::{Setup, Wanted};

/// A game to make: its setup, quest and dataset, and what it starts on.
pub struct Plan<'a> {
    install: &'a Install,
    setup: Setup,
    quest: Option<&'a Offered>,
    /// What its catalogue and text are made of: archives in load order,
    /// then the XML.
    sources: Sources,
    program: &'static Program,
    /// A quest's chart.
    chart: Option<PathBuf>,
    needs: Needs,
    /// A save of ours, going on.
    saved: Option<save::Snapshot>,
    /// A scene's file, read (`--scene`).
    scene: Option<Scene>,
}

/// A game made: the game, and what it is; the catalogue's art, if its
/// map was made.
pub struct Begun {
    pub sim: Sim,
    pub session: Session,
    pub art: Option<Art>,
}

/// The world a game is made on, before its game; a custom game's setup;
/// the catalogue's art, if its map was made.
struct Founded {
    world: World,
    setup: Option<freestyle::Setup>,
    art: Option<Art>,
}

impl<'a> Plan<'a> {
    /// The game `setup` asks for on `install`, `quest` its quest: in the
    /// quest's dataset, else the base install's.
    ///
    /// # Errors
    ///
    /// A dataset or program that will not read.
    pub fn of(
        install: &'a Install,
        setup: &Setup,
        quest: Option<&'a Offered>,
    ) -> Result<Plan<'a>, String> {
        let (hd, sdk) = (&install.hd, &install.sdk);
        let dataset = dataset::Install::new(hd, Bytecode::Sdk(sdk));
        let found = chosen(install, &setup.mods)?;
        let mods: Vec<&Mod> = found.iter().collect();

        let sets = quest.map_or(Datasets::Expansion, |o| Datasets::of(&o.quest));
        let files = match quest {
            Some(o) => dataset.quest_with(&o.quest, &mods),
            None => dataset.files_with(Datasets::Expansion, &mods),
        }
        .map_err(|e| e.to_string())?;

        // A mod for the other dataset is passed over
        // and not saved as needed.
        let applied: Vec<&Mod> = mods.into_iter().filter(|m| m.base.fits(sets)).collect();
        let mut sources = Sources::of(&files, hd);
        sources.flag_patches = applied
            .iter()
            .filter_map(|m| {
                m.inline_patch
                    .as_ref()
                    .map(|text| (m.manifest.clone(), text.clone()))
            })
            .collect();
        let chart = quest.map(|_| files.chart);
        let images = files.programs;
        let needs = needs(
            hd,
            &sources.archives,
            &images,
            (&applied, &install.builds()),
        )?;
        let scene = setup.scene.as_deref().map(scene).transpose()?;
        let program = crate::game::load::patched(&images, &sources.flag_patches)?;

        Ok(Plan {
            install,
            setup: setup.clone(),
            quest,
            sources,
            program,
            chart,
            needs,
            saved: None,
            scene,
        })
    }

    /// The game saved at `path` made again to go on: its session's setup
    /// makes the world as the saved game's was, in the dataset the setup
    /// names (`quests` the offered), the save's state taken over it. With
    /// its session, which a recording goes on from, so `play.input` still
    /// replays from the start, and its name.
    ///
    /// ```text
    ///  play.input's setup ──> Setup ──> Plan (founded) ──┐
    ///  snapshot.ron ──────────────────> Sim::resume <──┘
    /// ```
    ///
    /// # Errors
    ///
    /// A save that will not read, of another version, or one this
    /// install's archives or GPL images differ from.
    pub fn resumed(
        install: &'a Install,
        path: &Path,
        quests: &'a [Offered],
    ) -> Result<(Plan<'a>, record::Session, String), String> {
        let contents = save::read(path)?;

        // As HD's Mods Missing box: each mod
        // the save needs that no folder holds, named.
        let library = install.mods();
        let missing: Vec<String> = contents
            .meta
            .needs
            .mods
            .iter()
            .filter(|m| Guid::parse(&m.guid).and_then(|g| library.find(g)).is_none())
            .map(|m| format!("{} {}", m.name, m.guid))
            .collect();

        if !missing.is_empty() {
            Err(format!(
                "{}: the save needs mods not found: {}",
                path.display(),
                missing.join(", ")
            ))?;
        }

        let session = record::parse(&contents.play)?;
        let quest = crate::cli::quest_of(&session.setup, quests)?.map(|i| &quests[i]);
        let mut plan = Plan::of(install, &session.setup, quest)?;

        if let Some(why) = save::mismatch(&contents.meta.needs, &plan.needs) {
            Err(format!("{}: {why}", path.display()))?;
        }

        plan.saved = Some(contents.snapshot);
        Ok((plan, session, contents.meta.name))
    }

    /// What the game's catalogue and text are made of: archives, then a
    /// mod's or quest's XML.
    #[must_use]
    pub const fn sources(&self) -> &Sources {
        &self.sources
    }

    /// The game, on `catalogue` (made of [`Plan::sources`]): a save of
    /// ours going on, or set up by its quest's script, else the custom
    /// game's.
    ///
    /// # Errors
    ///
    /// A setup run that fails.
    pub fn begin(self, catalogue: &Catalogue<'_>) -> Result<Begun, String> {
        let Founded { world, setup, art } = self.founded(catalogue)?;

        let config = Config::default();
        let script = self
            .quest
            .map_or(startup::FREESTYLE, |q| q.quest.name.as_str());

        let (mut sim, resumed) = if let Some(saved) = self.saved {
            (
                Sim::resume(self.program, world, saved.game, saved.rewards, config)?,
                Some(Resumed {
                    interface: saved.interface,
                }),
            )
        } else {
            let begun = (self.setup.seed, script);
            let staging = match &self.scene {
                Some(scene) => scene.staging(&world).map_err(|e| e.to_string())?,
                None => Staging::default(),
            };

            (Sim::on(self.program, world, begun, &staging, config)?, None)
        };
        if let Some(cheats) = self.setup.cheats {
            sim.set_cheats(cheats);
        }
        let quest = self.quest.map(|q| (q.quest.key, q.title.clone()));

        // The title scroll's: a quest's goals, else the custom game's setup.
        let setup = setup.filter(|_| quest.is_none());
        let session = Session {
            quest,
            quest_id: self.quest.map(|q| q.quest.id),
            goals: self.quest.map(|q| q.quest.short.clone()),
            messages: self
                .quest
                .map(|q| q.quest.messages.clone())
                .unwrap_or_default(),
            setup,
            needs: self.needs,
            resumed,
        };

        Ok(Begun { sim, session, art })
    }

    /// The world the game is made on, before its game: a quest's own map,
    /// a scene's, else a new game from the picks.
    fn founded(&self, catalogue: &Catalogue<'_>) -> Result<Founded, String> {
        let (hd, program, setup) = (&self.install.hd, self.program, &self.setup);
        let on = |world, setup| Founded {
            world,
            setup,
            art: None,
        };

        match (self.quest, &self.chart) {
            (Some(q), Some(chart)) => {
                let begun = &q.quest;
                let art = Art::of(catalogue);
                let world = crate::game::load::quest_fresh(
                    (catalogue, &art),
                    program,
                    begun,
                    chart,
                    setup.word(),
                )?;

                Ok(Founded {
                    art: Some(art),
                    ..on(world, None)
                })
            }
            _ if let Some(scene) = &self.scene => {
                let art = Art::of(catalogue);
                let (world, made) =
                    crate::game::load::scene_world(hd, (catalogue, &art), program, scene)?;

                Ok(Founded {
                    art: Some(art),
                    ..on(world, Some(made))
                })
            }
            _ => {
                let picks = setup.picks();
                let art = Art::of(catalogue);
                let (world, made) = crate::game::load::fresh(
                    hd,
                    (catalogue, &art),
                    program,
                    (&picks, setup.events),
                    setup.gold,
                    setup.aim(),
                )?;

                // The setup's Building Limitations, as the build-tree
                // object holds them at the start (`World::limited`).
                Ok(Founded {
                    art: Some(art),
                    ..on(world.limited(&setup.caps()), Some(made))
                })
            }
        }
    }
}

/// The scene file at `path`, read.
fn scene(path: &Path) -> Result<Scene, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;

    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// What a save of a game on `archives`, `images` and `mods` needs of the
/// install `hd`; `builds` the mods compiled from source, whose images'
/// hash is needed too.
fn needs(
    hd: &Path,
    archives: &[PathBuf],
    images: &[PathBuf],
    (mods, builds): (&[&Mod], &Builds),
) -> Result<Needs, String> {
    let archives = archives
        .iter()
        .map(|a| a.strip_prefix(hd).unwrap_or(a).display().to_string())
        .collect();
    let mods = mods
        .iter()
        .map(|m| save::Needed {
            guid: m.guid.to_string(),
            name: m.title(),
            image: match builds.get(&m.guid) {
                Some(Built::Compiled { image, .. }) => Some(image.clone()),
                _ => None,
            },
            flags: m
                .inline_patch
                .as_ref()
                .map(|text| save::hash([text.as_bytes()])),
        })
        .collect();

    Ok(Needs {
        archives,
        images: crate::game::load::images_hash(images)?,
        mods,
    })
}

/// The mods `wanted` names, in order, from `install`'s folders.
///
/// # Errors
///
/// A GUID no folder holds; a mod whose GPL source did not compile; one
/// whose compiled images' hash is not the one asked for.
pub fn chosen(install: &Install, wanted: &[Wanted]) -> Result<Vec<Mod>, String> {
    let (library, builds) = (install.mods(), install.builds());

    wanted
        .iter()
        .map(|w| {
            let guid = w.guid;
            let m = library.find(guid).cloned().ok_or_else(|| {
                format!("mod {guid} is in none of the mod folders (the Mods page lists them)")
            })?;

            match (builds.get(&guid), &w.image) {
                (Some(Built::Refused(why)), _) => Err(format!(
                    "mod {} {guid}: its GPL source does not compile: {why}",
                    m.title()
                )),
                (Some(Built::Compiled { image, .. }), Some(asked)) if image != asked => {
                    Err(format!(
                        "mod {} {guid}: compiled from other GPL source than the game's \
                         (images {image}, the game's {asked})",
                        m.title()
                    ))
                }
                (None, Some(_)) => Err(format!(
                    "mod {} {guid}: the game's was compiled from GPL source, this one is not",
                    m.title()
                )),
                _ => Ok(m),
            }
        })
        .collect()
}

/// Where a made game's input is recorded.
pub enum Recording {
    /// `play.input` beside the log.
    Log,
    /// `play.input` in this directory: a test's.
    #[cfg_attr(not(test), expect(dead_code, reason = "the tests record apart"))]
    In(PathBuf),
    /// Nowhere: a test's.
    #[cfg_attr(not(test), expect(dead_code, reason = "the tests record apart"))]
    Off,
}

impl Recording {
    /// A recorder going on from `session`, if recorded.
    fn recorder(&self, session: &record::Session) -> Option<Recorder> {
        match self {
            Recording::Log => Some(Recorder::default_resumed(session)),
            Recording::In(dir) => Some(Recorder::in_dir(dir, session)),
            Recording::Off => None,
        }
    }

    /// `sim` recorded from its start, made by `setup`, `events` to play
    /// back.
    pub fn record(&self, sim: &mut Sim, setup: &Setup, events: Vec<record::Event>) {
        let session = record::Session {
            setup: setup.clone(),
            events: Vec::new(),
        };

        if let Some(recorder) = self.recorder(&session) {
            sim.record(recorder);
        }

        sim.replay(events);

        if let (Recording::Log, Some(path)) = (self, record::default_path()) {
            info!("recording to {}", path.display());
        }
    }
}

/// A screenshot's play before it is drawn: `--warm` frames, the bot's
/// presses in them, then the end `--end` asks for.
pub struct Warm {
    pub frames: u32,
    /// The bot's seed.
    pub bot: Option<u32>,
    pub end: Option<crate::draw::shot::Ending>,
}

/// The command line's game, made once the app is up.
pub struct Command {
    pub setup: Setup,
    /// This machine's player.
    pub local: u32,
    /// A screenshot's play before it is drawn; a screenshot records
    /// nothing.
    pub warm: Option<Warm>,
    pub events: Vec<record::Event>,
    pub offered: Option<Box<Offered>>,
    /// A LAN game's peer: the game is networked (`net`).
    pub peer: Option<crate::net::Peer>,
    /// A LAN game's save it goes on from, this machine's copy; none, a
    /// new game of `setup`.
    pub saved: Option<PathBuf>,
}

/// What a game is made from: a quest on a map word, a save (`quests` the
/// offered), the freestyle config, the command line, or a LAN game the
/// menus' lobby seated.
pub enum GameSource<'a> {
    Quest {
        offered: &'a Offered,
        /// The clock word its map is made from.
        word: i32,
    },
    Save {
        path: &'a Path,
        quests: &'a [Offered],
    },
    Freestyle(&'a Custom),
    Command,
    /// Made as the command line's would be, its peer's seat
    /// ([`Command::local`]) this machine's.
    Lan(Box<Command>),
}

/// A made game: its corpus, the game, and what it is; the corpus's art,
/// if its map was made.
pub struct Made {
    pub corpus: Corpus,
    pub sim: Sim,
    pub session: Session,
    pub art: Option<Art>,
}

/// Makes a picked quest's game, recorded as `--quest` records it; a
/// save's, recorded on from its session; a freestyle game as the custom
/// game's options make and record it; the command line's, once. Each on
/// the corpus the shelf holds for its dataset, if any.
#[derive(Resource)]
pub struct Maker {
    install: Install,
    command: Option<Command>,
    recording: Recording,
    /// This machine's player in every game it makes (`--local`).
    local: u32,
    /// The mods a menu's quest or freestyle game loads, in order: the
    /// settings' (`mod_list::chosen`).
    mods: Vec<String>,
}

impl Maker {
    #[must_use]
    pub const fn new(install: Install, command: Option<Command>, recording: Recording) -> Maker {
        Maker {
            install,
            command,
            recording,
            local: regent_core::sim::world::CROWN,
            mods: Vec::new(),
        }
    }

    /// The mods its menu games load from now: `enabled`, GUIDs in order.
    pub fn set_mods(&mut self, enabled: &[String]) {
        self.mods = enabled.to_vec();
    }

    /// The mods a menu game loads: those enabled that a folder holds
    /// and whose GPL source, if compiled, compiled; the rest warned of
    /// and left out, so the recording names what loaded.
    fn usable_mods(&self) -> Vec<Wanted> {
        let library = self.install.mods();
        let builds = self.install.builds();

        self.mods
            .iter()
            .filter_map(|text| {
                let guid = Guid::parse(text).filter(|&g| library.find(g).is_some());
                let refused = guid.and_then(|g| match builds.get(&g) {
                    Some(Built::Refused(why)) => Some(why),
                    _ => None,
                });

                match (guid, refused) {
                    (None, _) => warn!("mod {text}: not found; left out"),
                    (Some(_), Some(why)) => warn!("mod {text}: left out, refused: {why}"),
                    (Some(_), None) => {}
                }

                guid.filter(|_| refused.is_none()).map(Wanted::from)
            })
            .collect()
    }

    /// The same maker, `player` local in its games.
    #[must_use]
    pub const fn with_local(mut self, player: u32) -> Maker {
        self.local = player;
        self
    }

    /// The install it makes games on.
    #[must_use]
    pub const fn install(&self) -> &Install {
        &self.install
    }

    /// The game `source` asks for.
    ///
    /// # Errors
    ///
    /// A game that will not make, or the command line's asked twice.
    pub fn make(&mut self, source: GameSource<'_>, shelved: &mut Shelf) -> Result<Made, String> {
        // The menus' games take no command line options; their mods are
        // the settings'.
        let mods = self.usable_mods();
        let fresh = || Setup {
            mods: mods.clone(),
            ..Setup::default()
        };

        match source {
            GameSource::Quest { offered, word } => {
                let setup = Setup {
                    quest: Some(offered.named()),
                    map_seed: Some(word),
                    ..fresh()
                };

                self.drawn(
                    (&setup, None),
                    (Vec::new(), None),
                    (Some(offered), self.local),
                    shelved,
                )
            }
            GameSource::Save { path, quests } => {
                self.resumed((path, quests), (None, self.local), None, shelved)
            }
            GameSource::Freestyle(custom) => {
                let setup = fresh().freestyle(custom);

                self.drawn(
                    (&setup, None),
                    (Vec::new(), None),
                    (None, self.local),
                    shelved,
                )
            }
            GameSource::Command => match self.command.take() {
                Some(Command {
                    saved: Some(path),
                    peer,
                    setup,
                    local,
                    offered,
                    ..
                }) => {
                    let quests = offered.map(|o| vec![*o]).unwrap_or_default();

                    self.resumed((&path, &quests), (peer, local), setup.cheats, shelved)
                }
                Some(Command {
                    setup,
                    warm,
                    events,
                    offered,
                    peer,
                    saved: None,
                    ..
                }) => self.drawn(
                    (&setup, warm.as_ref()),
                    (events, peer),
                    (offered.as_deref(), self.local),
                    shelved,
                ),
                None => Err("the command line's game is made once".to_owned()),
            },
            GameSource::Lan(lan) => {
                let Command {
                    setup,
                    local,
                    warm,
                    events,
                    offered,
                    peer,
                    saved,
                } = *lan;

                // A save plays its own cheat rule on every peer: the
                // host's command holds its custom game's, a guest's the
                // save's setup.
                match saved {
                    Some(path) => self.resumed((&path, &[]), (peer, local), None, shelved),
                    None => self.drawn(
                        (&setup, warm.as_ref()),
                        (events, peer),
                        (offered.as_deref(), local),
                        shelved,
                    ),
                }
            }
        }
    }

    /// The game `setup` asks for, `offered` its quest, `local` this
    /// machine's player, recorded unless a screenshot (`shot` given),
    /// `events` played back; networked with a `peer`.
    fn drawn(
        &self,
        (setup, shot): (&Setup, Option<&Warm>),
        (events, peer): (Vec<record::Event>, Option<crate::net::Peer>),
        (offered, local): (Option<&Offered>, u32),
        shelved: &mut Shelf,
    ) -> Result<Made, String> {
        let plan = Plan::of(&self.install, setup, offered)?;
        let corpus = corpus(plan.sources(), shelved)?;
        let Begun {
            mut sim,
            session,
            art,
        } = plan.begin(&corpus.catalogue)?;

        if let Some(peer) = peer {
            sim = sim.networked(peer);
        }

        sim.set_local(local)?;

        // A screenshot records nothing.
        match shot {
            Some(asked) => {
                sim.replay(events);
                warm(&mut sim, asked, &corpus.catalogue);
            }
            None => {
                self.recording
                    .record(&mut sim, &setup.recorded().stamped(&session.needs), events);
            }
        }

        Ok(Made {
            corpus,
            sim,
            session,
            art,
        })
    }

    /// The game saved at `path` going on ([`Plan::resumed`]), `quests`
    /// the offered, recorded on from its session; `local` this machine's
    /// player, networked with a `peer`.
    fn resumed(
        &self,
        (path, quests): (&Path, &[Offered]),
        (peer, local): (Option<crate::net::Peer>, u32),
        cheats: Option<Cheats>,
        shelved: &mut Shelf,
    ) -> Result<Made, String> {
        let (plan, recorded, name) = Plan::resumed(&self.install, path, quests)?;
        let corpus = corpus(plan.sources(), shelved)?;
        let Begun {
            mut sim,
            session,
            art,
        } = plan.begin(&corpus.catalogue)?;

        if let Some(cheats) = cheats {
            sim.set_cheats(cheats);
        }

        if let Some(peer) = peer {
            sim = sim.networked(peer);
        }

        sim.set_local(local)?;

        info!(
            "loaded {name} ({}): frame {}, {} events",
            path.display(),
            sim.frames(),
            recorded.events.len()
        );

        if let Some(recorder) = self.recording.recorder(&recorded) {
            sim.record(recorder);
        }

        Ok(Made {
            corpus,
            sim,
            session,
            art,
        })
    }
}

/// The corpus on `sources`: the shelf's, else opened.
fn corpus(sources: &Sources, shelf: &mut Shelf) -> Result<Corpus, String> {
    shelf
        .take(&sources.files())
        .map_or_else(|| Corpus::with(sources), Ok)
}

/// A screenshot's `--warm` frames, played before drawing, the bot's
/// presses in them.
fn warm(sim: &mut Sim, warm: &Warm, catalogue: &Catalogue<'_>) {
    let mut bot = warm
        .bot
        .map(|seed| crate::game::bot::Bot::new(Menu::of(catalogue), seed));

    for _ in 0..warm.frames {
        if let Some(bot) = &mut bot {
            for input in bot.moves(sim) {
                sim.input(input);
            }
        }

        sim.step();
    }

    if let Some(ending) = warm.end {
        crate::draw::shot::end(sim, ending);
    }
}

/// The game `args` asks for, as every game is made, and its dataset's
/// catalogue: a test's.
#[cfg(test)]
pub fn test_game(args: &[&str]) -> (Begun, Catalogue<'static>) {
    test_setup(&crate::cli::tests::parsed(
        std::iter::once("openregent").chain(args.iter().copied()),
    ))
}

/// The same, of `setup`.
#[cfg(test)]
pub fn test_setup(setup: &Setup) -> (Begun, Catalogue<'static>) {
    let install = Install::from_env().expect("MAJESTY_HD");
    let quests = crate::quests(&install).expect("quests");
    let offered = crate::cli::quest_of(setup, &quests)
        .expect("its quest")
        .map(|i| &quests[i]);
    let plan = Plan::of(&install, setup, offered).expect("planned");
    let catalogue = regent_draw::data::catalogue_of(plan.sources()).expect("catalogue");

    (plan.begin(&catalogue).expect("begun"), catalogue)
}

/// The GPL program the game `args` make loads.
#[cfg(test)]
pub fn test_program(args: &[&str]) -> &'static Program {
    let setup =
        crate::cli::tests::parsed(std::iter::once("openregent").chain(args.iter().copied()));
    let install = Install::from_env().expect("MAJESTY_HD");
    let quests = crate::quests(&install).expect("quests");
    let offered = crate::cli::quest_of(&setup, &quests)
        .expect("its quest")
        .map(|i| &quests[i]);

    Plan::of(&install, &setup, offered)
        .expect("planned")
        .program
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::compiled::tests as compiled;

    /// A game with a compiled mod records its images' hash beside the
    /// GUID; the recording's setup makes the game again, and one naming
    /// another hash is refused, the mod named.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_recording_names_the_image_and_replays() {
        let (_dir, mods, _) = compiled::fixture("recorded");
        let install = crate::files::env::Install::from_env()
            .expect("MAJESTY_HD")
            .with_mod_folders(vec![mods]);
        let (image, _) = compiled::image(&install.builds()).expect("compiled");
        let cli = crate::cli::tests::parsed(["openregent", "--mods", compiled::GUID]);
        let plan = Plan::of(&install, &cli, None).expect("planned");
        let catalogue = regent_draw::data::catalogue_of(plan.sources()).expect("catalogue");
        let needs = plan.begin(&catalogue).expect("begun").session.needs;
        let setup = cli.recorded().stamped(&needs);
        let stamped = format!("{}@{image}", compiled::GUID);

        assert_eq!(needs.mods[0].image.as_deref(), Some(image.as_str()));
        assert_eq!(
            setup
                .mods
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            [stamped.as_str()]
        );

        // The recording's setup, its mods as `mods`, made again.
        let replay = |mods: &str| {
            let wanted = Wanted::parse(mods).expect("a mod");
            let setup = crate::setup::Setup {
                mods: vec![wanted],
                ..setup.clone()
            };

            Plan::of(&install, &setup, None).map(|_| ())
        };

        assert!(replay(&stamped).is_ok());
        assert!(
            replay(&format!("{}@0000000000000000", compiled::GUID))
                .is_err_and(|e| e.contains("Sourced") && e.contains("other GPL source"))
        );
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_new_game_uses_its_cheat_rule() {
        let setup = crate::cli::tests::parsed(["openregent", "--cheats", "false"]);
        let (begun, _) = test_setup(&setup);

        assert_eq!(begun.sim.world().cheats(), Cheats::Barred);
    }

    /// A LAN save loaded plays its own cheat rule, as the lobby shows
    /// it: the host's command, made of its custom game's options, does
    /// not change it; nor does a guest's, made of the save's setup.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_loaded_lan_save_keeps_its_cheat_rule() {
        let scratch = crate::testdata::scratch("begin-lan-cheats");
        let dir = scratch.path();

        // A game barring cheats, saved.
        let barred = crate::cli::tests::parsed(["openregent", "--cheats", "false"]);
        let (begun, catalogue) = test_setup(&barred);
        let Begun {
            mut sim, session, ..
        } = begun;

        Recording::In(dir.to_path_buf()).record(&mut sim, &barred.recorded(), Vec::new());
        sim.step();

        let interface = save::Interface {
            selected: None,
            reading: None,
            pan: (0.0, 0.0),
            zoom: 1.0,
            tracking: regent_core::view::panel::tracking::Tracker::default(),
        };
        let text = regent_core::data::text::Text::build(&catalogue);
        let game = save::FromSim::of(&sim);
        let contents = save::of((game, &session), &text, None, interface).expect("contents");
        let path = dir.join(format!("barred{}", save::EXTENSION));

        save::write(&path, &contents).expect("written");

        // Loaded on the LAN by a command allowing them.
        let install = Install::from_env().expect("MAJESTY_HD");
        let mut maker = Maker::new(install, None, Recording::Off);
        let command = Command {
            setup: crate::cli::tests::parsed(["openregent", "--cheats", "true"]),
            local: regent_core::sim::world::CROWN,
            warm: None,
            events: Vec::new(),
            offered: None,
            peer: None,
            saved: Some(path),
        };
        let made = maker
            .make(GameSource::Lan(Box::new(command)), &mut Shelf::default())
            .expect("made");

        assert_eq!(made.sim.world().cheats(), Cheats::Barred);
    }
}
