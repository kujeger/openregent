//! Loading a program: images in the engine's order, one function map.
//!
//! **Binary**, `MajestyHD.exe`:
//!
//! ```text
//! read the dataset definitions     Data/MajestyDatasetDefinitions.xml, then
//!   compose the dataset            the expansion's: the base's GPL list, then
//!                                  each own image not already listed
//! apply the dataset                on a GPL list change:
//!   register the builtins          the 166 builtins, first into the map
//!   load each image    × n         the list tail first
//!     read the .bcd                sections 1, 2, 3: a name already read is
//!                                  dropped, so the first image loaded wins
//!   link
//!     register the script          functions and constants into the map; a
//!                                  name present (a builtin) is skipped
//!     fix up                       function values must exist; #Name slots
//!                                  take a constant's value
//! ```
//!
//! Loading tail first with first wins means the last listed image owns a
//! shared name: the expansion over the base, `MX_Compatibility.bcd` over
//! `Bytecode.bcd`, a mod over both. [`dataset::load_order`] is that
//! reversal; [`Program::load`] takes images already in it.
//!
//! The dataset XML loader, which walks `datasets.xml`, is dead code in HD:
//! nothing calls it. The file still ships in `miscdata.cam`'s `XML1`;
//! [`dataset::datasets_gpl`] reads it.
//!
//! GPL has no global variables: every identifier is a parameter or a local,
//! and the one form the compiler leaves for an unknown name occurs in no
//! image. What a program holds by name is its functions, its constants, its
//! prototypes with their typed members, and its `.dat` instances.
//!
//! [`dataset::load_order`]: crate::content::dataset::load_order
//! [`dataset::datasets_gpl`]: crate::content::dataset::datasets_gpl

use std::path::PathBuf;

use crate::base::digest::Map;
use crate::content::{self, flags};
use crate::data::gpltype::TypeCode;

use super::builtins::BuiltinId;
use super::exec::{
    self, Callee, DataSet, Definition, Kind, Literal, Local, Names, Operand, Sym, Target,
};
use crate::data::error::Result;

/// A function map entry. The map is the engine's: builtins, script functions
/// and constants in one case-insensitive name space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Entry {
    /// Index into [`builtins::HD`](super::builtins::HD).
    Builtin(BuiltinId),
    /// Index into [`Program::definitions`]: a function.
    Script(u32),
    /// Index into [`Program::definitions`]: a `#Name` constant. Registered
    /// with no native function, so not callable.
    Constant(u32),
}

/// A name an image defined that an earlier loaded one already had.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shadow {
    pub name: Sym,
    /// The image whose definition was dropped, in load order.
    pub dropped: usize,
    /// The image whose definition stands.
    pub kept: usize,
}

/// What [`Program::unresolved`] reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Missing {
    /// A call's name is no builtin and no script function.
    Call(Sym),
    /// A function value names nothing in the map: the engine's
    /// `Error: Function $%s() not found`.
    Function(Sym),
    /// A `#Name` no script constant defines. The engine tries its named
    /// values and descriptors next, so this is for the host.
    Constant(Sym),
}

/// One unresolved reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Unresolved {
    /// Index into [`Program::definitions`].
    pub definition: u32,
    /// Instruction index.
    pub at: u32,
    pub missing: Missing,
}

/// A loaded, linked program. Read only: its function map indexes its
/// definitions.
#[derive(Clone, Debug, Default)]
pub struct Program {
    names: Names,
    definitions: Vec<Definition>,
    origin: Vec<usize>,
    data: Vec<DataSet>,
    shadowed: Vec<Shadow>,
    map: Map<String, Entry>,
    prototypes: Map<String, u32>,
    data_sets: Map<String, u32>,
}

/// How the engine compares names: gpl function map find and the
/// section readers ignore case.
fn fold(name: &str) -> String {
    name.to_ascii_lowercase()
}

impl Program {
    /// Load images, given in load order, and link them.
    ///
    /// # Errors
    ///
    /// An image [`exec::image`] cannot decode.
    ///
    /// # Panics
    ///
    /// Past `u32::MAX` definitions.
    pub fn load<B: AsRef<[u8]>>(images: &[B]) -> Result<Program> {
        let mut out = Program::default();

        for at in BuiltinId::all() {
            out.map.insert(fold(at.get().name), Entry::Builtin(at));
        }

        for (nth, bytes) in images.iter().enumerate() {
            let image = exec::image(bytes.as_ref(), &mut out.names)?;

            out.merge(image, nth);
        }

        out.register();
        out.link();

        Ok(out)
    }

    /// Add one image's definitions and data sets, keeping any name already
    /// read: gpl read code, gpl read data.
    fn merge(&mut self, image: exec::Image, nth: usize) {
        let mut seen: Map<String, u32> = self
            .definitions
            .iter()
            .zip(0..)
            .map(|(d, at)| (fold(self.names.get(d.name)), at))
            .collect();

        for definition in image.definitions {
            let key = fold(self.names.get(definition.name));

            if let Some(kept) = seen.get(&key) {
                self.shadowed.push(Shadow {
                    name: definition.name,
                    dropped: nth,
                    kept: self.origin[*kept as usize],
                });
            } else {
                let at = u32::try_from(self.definitions.len()).expect("under 2^32");

                seen.insert(key, at);
                self.definitions.push(definition);
                self.origin.push(nth);
            }
        }

        for set in image.data {
            let key = fold(self.names.get(set.name));
            let at = u32::try_from(self.data.len()).expect("under 2^32");

            if let std::collections::hash_map::Entry::Vacant(slot) = self.data_sets.entry(key) {
                slot.insert(at);
                self.data.push(set);
            }
        }
    }

    /// Gpl register script: functions and constants into the map,
    /// skipping a name present; prototypes apart.
    fn register(&mut self) {
        for (definition, at) in self.definitions.iter().zip(0u32..) {
            let key = fold(self.names.get(definition.name));

            match definition.kind {
                Kind::Function(_) => {
                    self.map.entry(key).or_insert(Entry::Script(at));
                }
                Kind::Constant(_) => {
                    self.map.entry(key).or_insert(Entry::Constant(at));
                }
                Kind::Prototype(_) => {
                    self.prototypes.entry(key).or_insert(at);
                }
            }
        }
    }

    /// Gpl fixups, and the call targets: a named callee becomes a map
    /// entry, a `#Name` a script constant's value.
    fn link(&mut self) {
        let values: Map<String, Literal> = self
            .definitions
            .iter()
            .filter_map(|d| match d.kind {
                Kind::Constant(value) => Some((fold(self.names.get(d.name)), value)),
                _ => None,
            })
            .collect();

        // Only registered constants: a shadowed one never reached the map.
        let values: Map<String, Literal> = values
            .into_iter()
            .filter(|(key, _)| matches!(self.map.get(key), Some(Entry::Constant(_))))
            .collect();

        let names = &self.names;
        let map = &self.map;

        for instr in self.definitions.iter_mut().flat_map(|d| &mut d.code) {
            for operand in [&mut instr.lhs, &mut instr.rhs] {
                if let Operand::Constant(name) = *operand
                    && let Some(value) = values.get(&fold(names.get(name)))
                {
                    *operand = Operand::Value(*value);
                }
            }

            if let Some(call) = instr.call.as_deref_mut()
                && let Callee::Name(name) = call.callee
            {
                call.callee = match map.get(&fold(names.get(name))) {
                    Some(Entry::Builtin(at)) => Callee::Fn(Target::Builtin(*at)),
                    Some(Entry::Script(at)) => Callee::Fn(Target::Script(*at)),
                    _ => Callee::Name(name),
                };
            }
        }
    }

    /// Every name the images spell.
    #[must_use]
    pub const fn names(&self) -> &Names {
        &self.names
    }

    /// Every definition that stands, first loaded first.
    #[must_use]
    pub fn definitions(&self) -> &[Definition] {
        &self.definitions
    }

    /// The image each definition came from, in load order.
    #[must_use]
    pub fn origin(&self) -> &[usize] {
        &self.origin
    }

    /// `.dat` instances that stand.
    #[must_use]
    pub fn data(&self) -> &[DataSet] {
        &self.data
    }

    /// Each definition a later loaded image dropped.
    #[must_use]
    pub fn shadowed(&self) -> &[Shadow] {
        &self.shadowed
    }

    /// The map entry for `name`, folding case.
    #[must_use]
    pub fn entry(&self, name: &str) -> Option<Entry> {
        self.map.get(&fold(name)).copied()
    }

    /// Names in the function map: what the engine registers.
    #[must_use]
    pub fn map_len(&self) -> usize {
        self.map.len()
    }

    /// The script function `name`.
    #[must_use]
    pub fn function(&self, name: &str) -> Option<&Definition> {
        match self.entry(name) {
            Some(Entry::Script(at)) => self.definitions.get(at as usize),
            _ => None,
        }
    }

    /// The value of constant `name`, sigil included.
    #[must_use]
    pub fn constant(&self, name: &str) -> Option<Literal> {
        match self.entry(name).map(|e| (e, self.definition_of(e))) {
            Some((Entry::Constant(_), Some(d))) => match d.kind {
                Kind::Constant(value) => Some(value),
                _ => None,
            },
            _ => None,
        }
    }

    /// The members of prototype `name`, with their types.
    #[must_use]
    pub fn prototype(&self, name: &str) -> Option<&[Local]> {
        let definition = self
            .prototypes
            .get(&fold(name))
            .and_then(|at| self.definitions.get(*at as usize));

        match definition.map(|d| &d.kind) {
            Some(Kind::Prototype(members)) => Some(members),
            _ => None,
        }
    }

    /// The `.dat` instance `name`.
    #[must_use]
    pub fn data_set(&self, name: &str) -> Option<&DataSet> {
        self.data_sets
            .get(&fold(name))
            .and_then(|at| self.data.get(*at as usize))
    }

    /// `.dat` instance `instance`'s text for `member` made `text`, as if
    /// its `.dat` said it: its first pair of that key, any case, else a
    /// new pair. openregent's, for a mod's `[[prototype]]`
    /// ([`Program::with_members`]). `false` when no instance has the name.
    pub fn set_text(&mut self, instance: &str, member: &str, text: &str) -> bool {
        let at = self.data_sets.get(&fold(instance)).copied();

        if let Some(at) = at {
            let (key, value) = (self.names.intern(member), self.names.intern(text));
            let names = &self.names;
            let set = &mut self.data[at as usize];
            let pair = set
                .values
                .iter_mut()
                .find(|(k, _)| names.get(*k).eq_ignore_ascii_case(member));

            match pair {
                Some((_, v)) => *v = value,
                None => set.values.push((key, value)),
            }

            true
        } else {
            false
        }
    }

    /// This program with `files`' `[[prototype]]` values set
    /// ([`flags::initials`]): each a `.dat` instance's text for a member,
    /// as if the `.dat` said it. Files and entries apply in order; a later
    /// one wins. `None` when no file sets any, so the program is the
    /// images' own.
    ///
    /// ```text
    ///  [[prototype]] instance = "GuardHouse2", set = { Max_Guards = 2 }
    ///    ──> GuardHouse2's (Max_Guards 2): every agent made or upgraded to it
    /// ```
    ///
    /// # Errors
    ///
    /// An instance no image has, a member its prototype lacks, a value the
    /// member's type cannot hold, or a list, coord or agent member; each
    /// in its file.
    pub fn with_members(
        &self,
        files: &[(PathBuf, String)],
    ) -> std::result::Result<Option<Program>, content::Error> {
        let mut out = None;

        for (path, initial) in flags::initials(files)? {
            let patched = out.get_or_insert_with(|| self.clone());

            patched.members(&initial).map_err(|e| e.at(path))?;
        }

        Ok(out)
    }

    /// `initial`'s values into its instance.
    fn members(&mut self, initial: &flags::Initial) -> std::result::Result<(), content::Error> {
        let instance = &initial.instance;
        let shape = self
            .data_set(instance)
            .map(|d| self.names.get(d.shape).to_owned())
            .ok_or_else(|| content::Error::UnknownInstance(instance.clone()))?;

        for (member, value) in &initial.set {
            let (name, ty) = self
                .prototype(&shape)
                .into_iter()
                .flatten()
                .map(|m| (self.names.get(m.name), m.ty))
                .find(|(name, _)| name.eq_ignore_ascii_case(member))
                .map(|(name, ty)| (name.to_owned(), ty))
                .ok_or_else(|| content::Error::UnknownMember {
                    instance: instance.clone(),
                    member: member.clone(),
                })?;

            // A function value must name one: the engine's own check would
            // make the null record of a name it lacks.
            let text = flags::text(ty, value)
                .filter(|t| ty != TypeCode::Function || self.entry(t).is_some())
                .ok_or_else(|| initial.bad(member, value))?;

            self.set_text(instance, &name, &text);
        }

        Ok(())
    }

    fn definition_of(&self, entry: Entry) -> Option<&Definition> {
        match entry {
            Entry::Script(at) | Entry::Constant(at) => self.definitions.get(at as usize),
            Entry::Builtin(_) => None,
        }
    }

    /// Every call, function value and `#Name` the map does not answer, in
    /// definition order.
    #[must_use]
    pub fn unresolved(&self) -> Vec<Unresolved> {
        let mut out = Vec::new();

        for (definition, nth) in self.definitions.iter().zip(0u32..) {
            for (instr, at) in definition.code.iter().zip(0u32..) {
                let mut push = |missing| {
                    out.push(Unresolved {
                        definition: nth,
                        at,
                        missing,
                    });
                };

                if let Some(Callee::Name(name)) = instr.call.as_deref().map(|c| c.callee) {
                    push(Missing::Call(name));
                }

                for operand in [instr.lhs, instr.rhs] {
                    match operand {
                        Operand::Value(Literal::Function(name))
                            if self.entry(self.names.get(name)).is_none() =>
                        {
                            push(Missing::Function(name));
                        }
                        Operand::Constant(name) => push(Missing::Constant(name)),
                        _ => {}
                    }
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builtins_alone_fill_the_map_with_166_and_the_own() {
        let program = Program::load::<&[u8]>(&[]).expect("no images");
        let own = crate::gpl::builtins::OWN.len();

        assert_eq!(program.map_len(), crate::gpl::builtins::HD.len() + own);
        assert!(program.entry("CREATEAGENT").is_some());
        assert_eq!(program.entry(""), Some(Entry::Builtin(BuiltinId(0))));
    }
}
