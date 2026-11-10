//! The static call graph of a GPL program, from its sources.
//!
//! A call names its callee, or reads it from a value: `(x's "a")(...)`. A
//! value call resolves to every function that can reach attribute `a`:
//! assigned as `$F`, copied through attributes, locals and the parameters of
//! direct calls, or named by a `.dat` member `a`. Anything else reaching `a`
//! makes it wild: every function whose value the program takes, or every
//! function once `$LookupFunction` turns a string into one. A parameter of a
//! function the engine dispatches, or whose value exists, takes anything.
//!
//! Flow-insensitive and blind to unit types: a cycle here is possible, not
//! shown.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use regent_core::gpl::ast::{AssignOp, Expr, ExprKind, FunctionDef, Item};
use regent_core::gpl::{dat, proj, runtime};

/// The builtin turning a string into a function value.
const LOOKUP: &str = "lookupfunction";

/// What a function value can come from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Func(String),
    Held(Holder),
    Wild,
}

/// Where a value call reads its callee.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Callee {
    Name(String),
    Attr(String),
    Other,
}

/// A variable that can hold a function value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Holder {
    Attr(String),
    Param(String, usize),
    Local(String, String),
}

#[derive(Debug, Default)]
pub struct Program {
    /// Every function, folded.
    pub defined: BTreeSet<String>,
    /// Per function, its callees.
    pub calls: BTreeMap<String, BTreeSet<Callee>>,
    /// Functions used as values: not called, or named by data.
    values: BTreeSet<String>,
    flows: BTreeMap<Holder, BTreeSet<Source>>,
    /// Parameter count per function.
    params: BTreeMap<String, usize>,
    lookup: bool,
}

fn fold(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// `x's "a"`, as `a`.
fn attr_of(e: &Expr<'_>) -> Option<String> {
    match &e.kind {
        ExprKind::Attr { attr, .. } => match attr.kind {
            ExprKind::Str(a) => Some(fold(a)),
            _ => None,
        },
        _ => None,
    }
}

/// The variable `v` names inside `f`: a parameter or a local.
fn variable(f: &FunctionDef<'_>, v: &str) -> Holder {
    match f.params.iter().position(|p| p.name.eq_ignore_ascii_case(v)) {
        Some(i) => Holder::Param(fold(f.name), i),
        None => Holder::Local(fold(f.name), fold(v)),
    }
}

/// Where the function value `e` computes can come from, inside `f`.
fn source(f: &FunctionDef<'_>, e: &Expr<'_>) -> Source {
    match &e.kind {
        ExprKind::Func(n) => Source::Func(fold(n)),
        ExprKind::Ident(v) => Source::Held(variable(f, v)),
        _ => attr_of(e).map_or(Source::Wild, |a| Source::Held(Holder::Attr(a))),
    }
}

impl Program {
    /// The program the listed projects compile to.
    pub fn load(projects: &[PathBuf]) -> Self {
        let mut p = Program::default();

        for path in projects {
            let text = std::fs::read_to_string(path).expect("a project file");
            let project = proj::parse(&text, path.parent().expect("a directory"));

            for source in &project.sources {
                p.add_source(&read(source));
            }

            for data in &project.data {
                p.add_data(&read(data));
            }
        }

        p.add_unknown_callers();
        p
    }

    /// A function the engine dispatches, or whose value exists, can be
    /// called with anything: a thread's arguments, a value call's.
    fn add_unknown_callers(&mut self) {
        let dispatched = runtime::DISPATCHED.iter().map(|n| fold(n));
        let open: BTreeSet<String> = self.values.iter().cloned().chain(dispatched).collect();

        for f in open {
            for i in 0..self.params.get(&f).copied().unwrap_or_default() {
                self.flows
                    .entry(Holder::Param(f.clone(), i))
                    .or_default()
                    .insert(Source::Wild);
            }
        }
    }

    fn add_source(&mut self, text: &str) {
        let parsed = regent_core::gpl::parse(text);
        assert!(parsed.is_ok(), "{:?}", parsed.errors);

        for item in &parsed.script.items {
            if let Item::Function(f) = item {
                self.add_function(item, f);
            }
        }
    }

    fn add_function(&mut self, item: &Item<'_>, f: &FunctionDef<'_>) {
        let me = fold(f.name);
        let mut callees = BTreeSet::new();
        let mut called_at = BTreeSet::new();

        self.defined.insert(me.clone());
        self.params.insert(me.clone(), f.params.len());

        // Calls first, so a callee is not also counted as a value.
        item.visit_exprs(&mut |e| {
            if let ExprKind::Call { callee, args } = &e.kind {
                called_at.insert((callee.span.start, callee.span.end));

                let to = match &callee.kind {
                    ExprKind::Func(n) => Callee::Name(fold(n)),
                    _ => attr_of(callee).map_or(Callee::Other, Callee::Attr),
                };

                // A direct call hands each argument to a parameter.
                if let Callee::Name(n) = &to {
                    self.lookup |= n == LOOKUP;

                    for (i, a) in args.iter().enumerate() {
                        self.flows
                            .entry(Holder::Param(n.clone(), i))
                            .or_default()
                            .insert(source(f, a));
                    }
                }

                callees.insert(to);
            }
        });

        item.visit_exprs(&mut |e| match &e.kind {
            ExprKind::Func(n) if !called_at.contains(&(e.span.start, e.span.end)) => {
                self.values.insert(fold(n));
            }
            ExprKind::Assign {
                op: AssignOp::Assign,
                lhs,
                rhs,
            } => {
                let into = match &lhs.kind {
                    ExprKind::Ident(v) => Some(variable(f, v)),
                    _ => attr_of(lhs).map(Holder::Attr),
                };

                if let Some(h) = into {
                    self.flows.entry(h).or_default().insert(source(f, rhs));
                }
            }
            _ => {}
        });

        self.calls.insert(me, callees);
    }

    /// A `.dat` member naming a function puts it in that attribute.
    fn add_data(&mut self, text: &str) {
        for proto in dat::parse(text).prototypes {
            for (key, value) in proto.values {
                if let dat::Value::Word(w) = value {
                    self.flows
                        .entry(Holder::Attr(fold(key)))
                        .or_default()
                        .insert(Source::Func(fold(w)));
                    self.values.insert(fold(w));
                }
            }
        }
    }

    /// Every function value that exists: all functions once a string can
    /// become one.
    fn top(&self) -> BTreeSet<String> {
        if self.lookup {
            self.defined.clone()
        } else {
            self.values.intersection(&self.defined).cloned().collect()
        }
    }

    /// The functions each holder can hold, or `None` when wild.
    fn resolve(&self) -> BTreeMap<Holder, Option<BTreeSet<String>>> {
        let mut out: BTreeMap<Holder, Option<BTreeSet<String>>> = self
            .flows
            .keys()
            .map(|h| (h.clone(), Some(BTreeSet::new())))
            .collect();

        // Grows monotonically, so the loop ends.
        loop {
            let mut changed = false;

            for (holder, sources) in &self.flows {
                let mut next = out[holder].clone();

                for s in sources {
                    // A holder nothing writes is empty.
                    let from = match s {
                        Source::Func(n) => Some(BTreeSet::from([n.clone()])),
                        Source::Wild => None,
                        Source::Held(h) => out.get(h).cloned().unwrap_or_default(),
                    };

                    next = match (next, from) {
                        (Some(mut have), Some(add)) => {
                            have.extend(add.intersection(&self.defined).cloned());
                            Some(have)
                        }
                        _ => None,
                    };
                }

                if next != out[holder] {
                    out.insert(holder.clone(), next);
                    changed = true;
                }
            }

            if !changed {
                break out;
            }
        }
    }

    /// Script function to script functions it can call, value calls
    /// resolved.
    pub fn graph(&self) -> BTreeMap<String, BTreeSet<String>> {
        let held = self.resolve();
        let top = self.top();

        let targets = |c: &Callee| -> BTreeSet<String> {
            match c {
                Callee::Name(n) => BTreeSet::from([n.clone()]),
                Callee::Attr(a) => match held.get(&Holder::Attr(a.clone())) {
                    Some(Some(set)) => set.clone(),
                    Some(None) => top.clone(),
                    None => BTreeSet::new(),
                },
                Callee::Other => top.clone(),
            }
        };

        self.calls
            .iter()
            .map(|(f, cs)| {
                let to = cs
                    .iter()
                    .flat_map(targets)
                    .filter(|t| self.defined.contains(t))
                    .collect();

                (f.clone(), to)
            })
            .collect()
    }

    /// Attributes a value call reads, and whether each is wild.
    pub fn called_attributes(&self) -> BTreeMap<String, bool> {
        let held = self.resolve();

        self.calls
            .values()
            .flatten()
            .filter_map(|c| match c {
                Callee::Attr(a) => Some(a.clone()),
                _ => None,
            })
            .map(|a| {
                let wild = matches!(held.get(&Holder::Attr(a.clone())), Some(None));
                (a, wild)
            })
            .collect()
    }
}

fn read(path: &std::path::Path) -> String {
    let path =
        regent_core::content::on_disk(path).unwrap_or_else(|| panic!("{} on disk", path.display()));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

    String::from_utf8_lossy(&bytes).into_owned()
}

/// The strongly connected components with a cycle: more than one function,
/// or one calling itself. Tarjan's algorithm, iterative.
pub fn cycles(graph: &BTreeMap<String, BTreeSet<String>>) -> Vec<BTreeSet<String>> {
    let names: Vec<&String> = graph.keys().collect();
    let index_of: BTreeMap<&String, usize> =
        names.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let edges: Vec<Vec<usize>> = names
        .iter()
        .map(|n| {
            graph[*n]
                .iter()
                .filter_map(|t| index_of.get(t).copied())
                .collect()
        })
        .collect();

    let mut index = vec![usize::MAX; names.len()];
    let mut low = vec![0; names.len()];
    let mut on_stack = vec![false; names.len()];
    let mut stack = Vec::new();
    let mut next = 0;
    let mut out = Vec::new();

    for root in 0..names.len() {
        if index[root] != usize::MAX {
            continue;
        }

        // (node, next edge to try)
        let mut work = vec![(root, 0)];

        while let Some((v, e)) = work.pop() {
            if e == 0 {
                index[v] = next;
                low[v] = next;
                next += 1;
                stack.push(v);
                on_stack[v] = true;
            }

            if let Some(&w) = edges[v].get(e) {
                work.push((v, e + 1));

                if index[w] == usize::MAX {
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }

                continue;
            }

            // v is done: fold its low into the parent's, pop a root.
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[v]);
            }

            if low[v] == index[v] {
                let mut component = BTreeSet::new();

                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    component.insert(names[w].clone());

                    if w == v {
                        break;
                    }
                }

                if component.len() > 1 || edges[v].contains(&v) {
                    out.push(component);
                }
            }
        }
    }

    out
}
