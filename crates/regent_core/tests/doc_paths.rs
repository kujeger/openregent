//! Every Rust path and `.rs` file the notes name exists, so a moved or
//! renamed item leaves no stale mention behind; and every link between
//! them lands (`tools/check_links.py`).
//!
//! ```text
//!  crates/*/src/**.rs ──> modules: path ──> names defined or re-exported
//!                       types:   name ──> files defining or impl-ing it
//!  **.md, `…` spans ──> a::b::C ──> a module ending a::b, its item C
//!                                   else a type C, its member
//!                   ──> x/y.rs  ──> a file under the repo or a crate
//! ```
//!
//! A heuristic, lenient on purpose: a path is checked only when its first
//! segment is one of our modules, crates or types, so a C++ name from the
//! binary (`GE_Unit::Think`) passes unread; a member (`World::step`)
//! must be declared in a file that defines or implements the type.
//! Needs no game data.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Item keywords a definition line starts with, past its visibility.
const KEYWORDS: [&str; 13] = [
    "const fn",
    "async fn",
    "unsafe fn",
    "fn",
    "struct",
    "enum",
    "const",
    "static",
    "type",
    "trait",
    "mod",
    "union",
    "macro_rules!",
];

/// Keywords that define a type, for members.
const TYPE_KEYWORDS: [&str; 5] = ["struct", "enum", "trait", "type", "union"];

/// What a path segment separates on.
const SEP: &str = "::";

/// A module: the names it defines or re-exports, and its glob
/// re-exports' paths.
#[derive(Default)]
struct Module {
    names: HashSet<String>,
    globs: Vec<Vec<String>>,
}

/// Our Rust sources, indexed.
#[derive(Default)]
struct Tree {
    /// By path, the crate first: `["regent_core", "sim", "host"]`.
    modules: HashMap<Vec<String>, Module>,
    /// File indexes into `texts` by type name.
    types: HashMap<String, HashSet<usize>>,
    texts: Vec<String>,
    /// Every module's last segment, and the crates.
    names: HashSet<String>,
    /// Every crate directory.
    crates: Vec<PathBuf>,
    /// Every `.rs` under them, as text.
    files: Vec<String>,
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Files under `dir` with extension `ext`; symlinks, hidden directories
/// and `target` left out (the game data links, the worktrees).
fn walk(dir: &Path, ext: &str) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| !t.is_symlink()))
        .map(|e| e.path())
        .flat_map(|p| {
            let name = p
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());

            if p.is_dir() && !name.starts_with('.') && name != "target" {
                walk(&p, ext)
            } else if p.extension().is_some_and(|x| x == ext) {
                vec![p]
            } else {
                Vec::new()
            }
        })
        .collect()
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The leading identifier of `rest`.
fn ident(rest: &str) -> String {
    rest.chars().take_while(|c| is_ident(*c)).collect()
}

/// `line` past `pub` or `pub(…)` and leading space.
fn unvisible(line: &str) -> &str {
    let line = line.trim_start();

    match line.strip_prefix("pub") {
        Some(rest) if rest.starts_with('(') => rest
            .split_once(')')
            .map_or(rest, |(_, after)| after)
            .trim_start(),
        Some(rest) if rest.starts_with(' ') => rest.trim_start(),
        _ => line,
    }
}

/// The item a line defines, and its keyword: `pub fn step(` is
/// `("fn", "step")`.
fn defined(line: &str) -> Option<(&'static str, String)> {
    let line = unvisible(line);

    KEYWORDS.iter().find_map(|k| {
        let rest = line.strip_prefix(k)?;
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        let name = ident(rest);

        (!name.is_empty() && (k.ends_with('!') || line[k.len()..].starts_with(' ')))
            .then_some((*k, name))
    })
}

/// `text` with each `<…>`, nested too, cut.
fn ungeneric(text: &str) -> String {
    let mut depth = 0usize;
    let mut out = String::new();

    for c in text.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }

    out
}

/// The type an `impl` line implements: `impl Host for Replay<'_> {` is
/// `Replay`.
fn implemented(line: &str) -> Option<String> {
    let rest = ungeneric(line.trim_start().strip_prefix("impl")?);
    let target = rest.rsplit_once(" for ").map_or(rest.as_str(), |(_, t)| t);
    let target = target.trim_start().trim_start_matches(['&', ' ']);
    let path: String = target
        .chars()
        .take_while(|c| is_ident(*c) || *c == ':')
        .collect();
    let name = path.rsplit(SEP).next().unwrap_or_default();

    (!name.is_empty()).then(|| name.to_owned())
}

/// The names a `pub use` brings in, and its glob paths: `a::{b, c as d}`
/// is `b` and `d`; `a::*` the glob `a`.
fn exported(spec: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let flat = spec.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut names = Vec::new();
    let mut globs = Vec::new();

    for part in flat
        .split(['{', '}', ','])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let segments: Vec<&str> = part.split(SEP).map(str::trim).collect();

        match (part.split_once(" as "), segments.last()) {
            (Some((_, alias)), _) => names.push(alias.trim().to_owned()),
            (None, Some(&"*")) => globs.push(
                segments[..segments.len() - 1]
                    .iter()
                    .filter(|s| !matches!(**s, "crate" | "super" | "self" | ""))
                    .map(|s| (*s).to_owned())
                    .collect(),
            ),
            (None, Some(&"self")) if segments.len() > 1 => {
                names.push(segments[segments.len() - 2].to_owned());
            }
            (None, Some(last)) => names.push((*last).to_owned()),
            (None, None) => {}
        }
    }

    (names, globs)
}

impl Tree {
    fn read() -> Tree {
        let crates_dir = repo().join("crates");
        let mut tree = Tree::default();

        // Each crate: a directory with a `Cargo.toml` and a `src`.
        let mut crates: Vec<PathBuf> = std::fs::read_dir(&crates_dir)
            .expect("crates/")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("Cargo.toml").is_file() && p.join("src").is_dir())
            .collect();

        crates.sort();

        for dir in &crates {
            let name = dir
                .file_name()
                .expect("named")
                .to_string_lossy()
                .replace('-', "_");
            let src = dir.join("src");

            tree.names.insert(name.clone());

            for file in walk(&src, "rs") {
                tree.add(&name, &src, &file);
            }

            tree.files.extend(
                walk(dir, "rs")
                    .iter()
                    .map(|f| f.to_string_lossy().into_owned()),
            );
        }

        tree.crates = crates;
        tree
    }

    /// `file`, under crate `name`'s `src`.
    fn add(&mut self, name: &str, src: &Path, file: &Path) {
        let relative = file
            .strip_prefix(src)
            .expect("under src")
            .with_extension("");
        let mut path: Vec<String> = std::iter::once(name.to_owned())
            .chain(
                relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned()),
            )
            .collect();

        if path.len() > 1
            && matches!(
                path.last().map(String::as_str),
                Some("mod" | "lib" | "main")
            )
        {
            path.pop();
        }

        let text = std::fs::read_to_string(file).expect("source");
        let index = self.texts.len();
        let mut module = Module::default();
        let mut inline = Vec::new();

        // `pub use` statements run to their `;`.
        let mut using: Option<String> = None;

        for line in text.lines() {
            if let Some(spec) = using.as_mut() {
                spec.push_str(line);
            } else if unvisible(line).starts_with("use ") && line.trim_start().starts_with("pub") {
                using = Some(unvisible(line)["use ".len()..].to_owned());
            }

            if let Some(spec) = using.take_if(|s| s.contains(';')) {
                let (names, globs) = exported(spec.split(';').next().unwrap_or_default());

                module.names.extend(names);
                module.globs.extend(globs);
            }

            if let Some((keyword, item)) = defined(line) {
                if keyword == "mod" && line.trim_end().ends_with('{') {
                    inline.push(item.clone());
                }

                if TYPE_KEYWORDS.contains(&keyword) {
                    self.types.entry(item.clone()).or_default().insert(index);
                }

                module.names.insert(item);
            }

            if let Some(typ) = implemented(line) {
                self.types.entry(typ).or_default().insert(index);
            }
        }

        // An inline module's names: the file's, a superset.
        for name in inline {
            let mut sub = path.clone();

            sub.push(name.clone());
            self.names.insert(name);
            self.modules
                .entry(sub)
                .or_default()
                .names
                .extend(module.names.iter().cloned());
        }

        if let Some(last) = path.last() {
            self.names.insert(last.clone());
        }

        let entry = self.modules.entry(path).or_default();

        entry.names.extend(module.names);
        entry.globs.extend(module.globs);
        self.texts.push(text);
    }

    /// Modules whose path ends with the most of `segments`' head, and how
    /// many: `game::UI_RUNS` finds `replay::game`, 1.
    fn ending(&self, segments: &[String]) -> Vec<(usize, &Vec<String>)> {
        (1..=segments.len())
            .rev()
            .map(|k| {
                self.modules
                    .keys()
                    .filter(|p| p.ends_with(&segments[..k]))
                    .map(|p| (k, p))
                    .collect::<Vec<_>>()
            })
            .find(|found| !found.is_empty())
            .unwrap_or_default()
    }

    /// Whether `rest` names something in module `path`.
    fn within(&self, path: &[String], rest: &[String]) -> bool {
        let module = &self.modules[path];
        let sub = |item: &String| [path, std::slice::from_ref(item)].concat();

        match rest {
            [] => true,
            [item, tail @ ..] if module.names.contains(item) => {
                let inner = sub(item);

                tail.is_empty()
                    || (self.modules.contains_key(&inner) && self.within(&inner, tail))
                    || self.member(item, tail)
            }
            [item, tail @ ..] => {
                let inner = sub(item);

                (self.modules.contains_key(&inner) && self.within(&inner, tail))
                    || module.globs.iter().any(|g| {
                        self.ending(g)
                            .iter()
                            .filter(|(k, _)| *k == g.len())
                            .any(|(_, p)| self.within(p, rest))
                    })
            }
        }
    }

    /// Whether `rest` is a member of type `typ`: declared in a file that
    /// defines or implements it. A C++ virtual's RTTI name passes.
    fn member(&self, typ: &str, rest: &[String]) -> bool {
        match rest {
            [] => true,
            [last] if is_virtual(last) => true,
            [last] => self
                .types
                .get(typ)
                .is_some_and(|files| files.iter().any(|f| declares(&self.texts[*f], last))),
            [next, tail @ ..] => next.starts_with(char::is_uppercase) && self.member(next, tail),
        }
    }

    fn resolves(&self, segments: &[String]) -> bool {
        let segments = match segments {
            [first, rest @ ..] if matches!(first.as_str(), "crate" | "self" | "super") => rest,
            all => all,
        };

        segments.is_empty()
            || self
                .ending(segments)
                .iter()
                .any(|(k, p)| self.within(p, &segments[*k..]))
            || self.member(&segments[0], &segments[1..]) && self.types.contains_key(&segments[0])
            || (1..segments.len()).rev().any(|k| {
                self.modules
                    .keys()
                    .filter(|p| p.ends_with(&segments[..k]))
                    .any(|p| self.within(p, &segments[k..]))
            })
    }

    /// Whether a path's first segment is ours to check.
    fn ours(&self, first: &str) -> bool {
        self.names.contains(first)
            || self.types.contains_key(first)
            || matches!(first, "crate" | "self" | "super")
    }

    /// Whether `.rs` file `relative`, named in note `note`, exists: under
    /// the repo, the note's directory, `crates/`, a crate or its `src`, else
    /// as the tail of any of our files.
    fn file(&self, relative: &str, note: &Path) -> bool {
        if let Some((head, rest)) = relative.split_once('{') {
            let (choices, tail) = rest.split_once('}').unwrap_or((rest, ""));

            choices
                .split(',')
                .all(|c| self.file(&format!("{head}{}{tail}", c.trim()), note))
        } else {
            let root = repo();
            let bases = [
                root.clone(),
                note.parent().unwrap_or(&root).to_path_buf(),
                root.join("crates"),
            ]
            .into_iter()
            .chain(self.crates.iter().flat_map(|c| [c.clone(), c.join("src")]));
            let tail = format!("/{relative}");

            bases.into_iter().any(|b| b.join(relative).exists())
                || self.files.iter().any(|f| f.ends_with(&tail))
        }
    }
}

fn is_virtual(name: &str) -> bool {
    name.strip_prefix("vf")
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// Whether `text` declares `word` as a member: a method, an associated
/// item, a field or a variant, at the start of its line.
fn declares(text: &str, word: &str) -> bool {
    text.lines().any(|line| {
        let line = unvisible(line);
        let next = line.strip_prefix(word).and_then(|rest| rest.chars().next());

        defined(line).is_some_and(|(_, name)| name == word)
            || matches!(next, Some(',' | '(' | '{' | ' ' | '='))
            || (next == Some(':') && !line[word.len()..].starts_with(SEP))
    })
}

/// A backticked span as a path: call parentheses, generics and a macro's
/// `!` cut; `a::{b, c}` as `a::b` and `a::c`.
fn paths(span: &str) -> Vec<String> {
    let span = span.trim();
    let span = span.split_once('(').map_or(span, |(head, _)| head);
    let span = ungeneric(span);
    let span = span.trim_end_matches('!');

    match span.split_once("::{") {
        Some((head, rest)) => rest
            .trim_end_matches('}')
            .split(',')
            .map(|c| format!("{head}{SEP}{}", c.trim()))
            .collect(),
        None => vec![span.to_owned()],
    }
}

/// `path`'s segments if it reads as a Rust path: two or more identifiers,
/// the first may be a crate's hyphenated name.
fn segments(path: &str) -> Option<Vec<String>> {
    let segments: Vec<String> = path.split(SEP).map(str::to_owned).collect();
    let first = segments[0].replace('-', "_");
    let well_formed = segments.len() > 1
        && segments
            .iter()
            .all(|s| !s.is_empty() && s.chars().all(is_ident))
        && first.chars().all(is_ident)
        && first.starts_with(|c: char| c.is_alphabetic() || c == '_');

    well_formed.then(|| [vec![first], segments[1..].to_vec()].concat())
}

fn is_rs_file(span: &str) -> bool {
    Path::new(span).extension().is_some_and(|x| x == "rs")
        && span
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./{},-".contains(c))
}

/// Each backticked span on `line`.
fn spans(line: &str) -> impl Iterator<Item = &str> {
    line.split('`').skip(1).step_by(2)
}

#[test]
fn every_rust_path_in_the_notes_exists() {
    let tree = Tree::read();
    let root = repo();
    let mut stale = Vec::new();
    // `local/` is private and unpublished: its notes may name old code.
    let private = root.join("local");
    let mut notes: Vec<PathBuf> = walk(&root, "md")
        .into_iter()
        .filter(|n| !n.starts_with(&private))
        .collect();

    notes.sort();

    for note in notes {
        let text = std::fs::read_to_string(&note).expect("note");
        let shown = note
            .strip_prefix(&root)
            .unwrap_or(&note)
            .display()
            .to_string();

        for (n, line) in text.lines().enumerate() {
            for span in spans(line) {
                let bad = if is_rs_file(span.trim()) {
                    !tree.file(span.trim(), &note)
                } else {
                    paths(span)
                        .iter()
                        .filter_map(|p| segments(p))
                        .filter(|s| tree.ours(&s[0]))
                        .any(|s| !tree.resolves(&s))
                };

                if bad {
                    stale.push(format!("{shown}:{}: `{span}`", n + 1));
                }
            }
        }
    }

    assert!(
        stale.is_empty(),
        "paths that name nothing:\n{}",
        stale.join("\n")
    );
}

/// Every markdown link between the notes lands on a file and a heading:
/// `tools/check_links.py`, GitHub's anchors.
#[test]
fn every_link_between_the_notes_lands() {
    let root = repo();
    let out = std::process::Command::new("python3")
        .arg("-I")
        .arg(root.join("tools/check_links.py"))
        .arg(&root)
        .output()
        .expect("python3 runs");

    assert!(
        out.status.success(),
        "broken links:\n{}",
        String::from_utf8_lossy(&out.stdout)
    );
}
