//! Cross-checks a parsed corpus.
//!
//! Does every `$Name` refer to something that exists? Used to:
//!
//! - validate the parser: a call to a function nobody defines suggests a
//!   misparse;
//! - yield the builtin inventory an interpreter needs.
//!
//! GPL names are case-insensitive, so everything here folds case.

use std::collections::{BTreeMap, BTreeSet};

use super::ast::{Expr, ExprKind, Item, Script};
use super::builtins;

#[derive(Debug, Default)]
pub struct Report<'a> {
    /// Function definitions, folded name to original spelling.
    pub defined: BTreeMap<String, &'a str>,
    /// Every `$Name` referenced, folded name to use count.
    pub referenced: BTreeMap<String, usize>,
    /// Prototype definitions, for the `.dat` cross-check.
    pub prototypes: BTreeSet<String>,
    /// Definitions sharing a name, compared case-insensitively as GPL does.
    pub duplicate_definitions: Vec<String>,
}

impl<'a> Report<'a> {
    /// Fold one parsed file's definitions and references into the report.
    pub fn add(&mut self, script: &Script<'a>) {
        for item in &script.items {
            match item {
                Item::Function(f) => {
                    let key = f.name.to_ascii_lowercase();

                    if self.defined.insert(key.clone(), f.name).is_some() {
                        self.duplicate_definitions.push(key);
                    }
                }
                Item::Prototype(p) => {
                    self.prototypes.insert(p.name.to_ascii_lowercase());
                }
                Item::Constant(_) => {}
            }
        }

        script.visit_exprs(&mut |e: &Expr<'_>| {
            if let ExprKind::Func(name) = e.kind {
                *self
                    .referenced
                    .entry(name.to_ascii_lowercase())
                    .or_default() += 1;
            }
        });
    }

    fn builtins() -> BTreeSet<String> {
        builtins::documented()
            .map(str::to_ascii_lowercase)
            .collect()
    }

    /// Referenced names matching neither a corpus function nor a documented
    /// builtin, with use counts, most-used first.
    pub fn unresolved(&self) -> Vec<(&str, usize)> {
        let builtins = Self::builtins();

        let mut out: Vec<(&str, usize)> = self
            .referenced
            .iter()
            .filter(|(name, _)| !self.defined.contains_key(*name) && !builtins.contains(*name))
            .map(|(name, n)| (name.as_str(), *n))
            .collect();

        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        out
    }

    /// Documented builtins the corpus never calls.
    pub fn unused_builtins(&self) -> Vec<&'static str> {
        builtins::documented()
            .filter(|b| !self.referenced.contains_key(&b.to_ascii_lowercase()))
            .collect()
    }

    /// Corpus functions nothing references. Non-empty: the engine calls entry
    /// points itself.
    pub fn uncalled(&self) -> Vec<&'a str> {
        self.defined
            .iter()
            .filter(|(name, _)| !self.referenced.contains_key(*name))
            .map(|(_, spelling)| *spelling)
            .collect()
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        let builtins = Self::builtins();
        let mut script = 0;
        let mut builtin = 0;
        let mut unresolved = 0;

        for (name, n) in &self.referenced {
            if self.defined.contains_key(name) {
                script += n;
            } else if builtins.contains(name) {
                builtin += n;
            } else {
                unresolved += n;
            }
        }

        (script, builtin, unresolved)
    }
}
