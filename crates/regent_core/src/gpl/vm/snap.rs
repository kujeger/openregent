//! The VM between runs, to and from the save model.

use super::*;
use crate::data::fnv;
use crate::save::{self as m, Refused, Saved};
use std::result::Result;

impl Saved for Thread {
    type Model = m::Thread;

    fn save(&self) -> m::Thread {
        let Thread {
            frame,
            interval,
            mode,
            suspended,
            left,
            running,
            pending,
            last,
            args,
        } = self;

        m::Thread {
            frame: frame.clone(),
            interval: *interval,
            mode: mode.save(),
            suspended: *suspended,
            left: *left,
            running: *running,
            pending: *pending,
            last: *last,
            args: args.save(),
        }
    }

    fn load(model: m::Thread) -> Result<Thread, Refused> {
        Ok(Thread {
            frame: model.frame,
            interval: model.interval,
            mode: ThreadMode::load(model.mode)?,
            suspended: model.suspended,
            left: model.left,
            running: model.running,
            pending: model.pending,
            last: model.last,
            args: Vec::load(model.args)?,
        })
    }
}

impl Saved for ObjId {
    type Model = u32;

    fn save(&self) -> u32 {
        self.0
    }

    fn load(model: u32) -> Result<ObjId, Refused> {
        Ok(ObjId(model))
    }
}

impl Saved for Cursor {
    type Model = m::Cursor;

    fn save(&self) -> m::Cursor {
        match *self {
            Cursor::Free => m::Cursor::Free,
            Cursor::At { list, node } => m::Cursor::At {
                list: list.0,
                node: ops::node(node),
            },
            Cursor::End => m::Cursor::End,
        }
    }

    fn load(model: m::Cursor) -> Result<Cursor, Refused> {
        Ok(match model {
            m::Cursor::Free => Cursor::Free,
            m::Cursor::At { list, node } => Cursor::At {
                list: ObjId(list),
                node: ops::unnode(node)?,
            },
            m::Cursor::End => Cursor::End,
        })
    }
}

/// What a saved function handle and activation depend on: the
/// builtins' count, then each definition's name and record count, in
/// order. FNV-1a, so stable across builds.
fn fingerprint(program: &Program) -> u64 {
    let defs = program.definitions().iter().flat_map(|d| {
        let name = program.names().get(d.name).bytes();

        name.chain([0])
            .chain(index(d.code.len()).to_le_bytes())
            .collect::<Vec<u8>>()
    });

    fnv::fold(
        fnv::START,
        FIRST_DEFINITION.to_le_bytes().into_iter().chain(defs),
    )
}

impl Vm<'_> {
    /// The VM between runs, as a save holds it.
    #[must_use]
    pub fn saved(&self) -> m::Vm {
        // Program and settings are the session's; the call stack is
        // empty between runs; the rest are diagnostics.
        let Vm {
            program: _,
            config: _,
            objects,
            threads,
            free,
            activations,
            top,
            frames: _,
            depth: _,
            error_line,
            count,
            rng,
            limit: _,
            steps,
            unwound: _,
            longest: _,
            trace: _,
            journal: _,
        } = self;
        let activations = activations
            .iter()
            .enumerate()
            .filter_map(|(def, a)| Some((u32::try_from(def).ok()?, a.as_ref()?)))
            .map(|(def, a)| {
                let Activation {
                    handles,
                    links: _,
                    cursors,
                } = a;

                (
                    def,
                    m::Activation {
                        handles: handles.save(),
                        cursors: cursors.save(),
                    },
                )
            })
            .collect();

        m::Vm {
            program: fingerprint(self.program),
            objects: objects.save(),
            threads: threads.save(),
            free: free.clone(),
            activations,
            top: top.save(),
            error_line: *error_line,
            count: *count,
            rng: *rng,
            steps: *steps,
        }
    }

    /// Take the saved VM's state; its links rebuilt from the program.
    ///
    /// # Errors
    ///
    /// A save of another program; an activation of a definition the
    /// program lacks, or of another length; an object id past the
    /// objects.
    pub fn restore(&mut self, model: m::Vm) -> Result<(), Refused> {
        let program = self.program;
        let mut activations = vec![None; program.definitions().len()];

        if model.program != fingerprint(program) {
            Err(Refused::Program)?;
        }

        held_ids(&model)?;

        for (def, a) in model.activations {
            let definition = program
                .definitions()
                .get(def as usize)
                .ok_or_else(|| Refused::Missing(format!("definition {def}")))?;
            let records = index(definition.code.len());

            let parts = definition.code.len() * PARTS as usize;

            if a.cursors.len() != definition.code.len() || a.handles.len() != parts {
                Err(Refused::Definition(def))?;
            }

            activations[def as usize] = Some(Activation {
                handles: Vec::load(a.handles)?,
                links: (0..records).map(|at| link(definition, at)).collect(),
                cursors: Vec::load(a.cursors)?,
            });
        }

        self.objects = Vec::load(model.objects)?;
        self.threads = Map::load(model.threads)?;
        self.free = model.free;
        self.activations = activations;
        self.top = Vec::load(model.top)?;
        self.frames.clear();
        self.depth = 0;
        self.error_line = model.error_line;
        self.count = model.count;
        self.rng = model.rng;
        self.steps = model.steps;
        Ok(())
    }
}

/// Every object id `model` holds outside its objects names one of them:
/// handles, top slots, the free list, threads and cursors.
fn held_ids(model: &m::Vm) -> Result<(), Refused> {
    let activations = model.activations.values();
    let handles = activations.clone().flat_map(|a| a.handles.iter().flatten());
    let cursors = activations
        .flat_map(|a| a.cursors.iter())
        .filter_map(|c| match c {
            m::Cursor::At { list, .. } => Some(list),
            m::Cursor::Free | m::Cursor::End => None,
        });
    let ids = handles
        .chain(model.top.iter().flatten())
        .chain(&model.free)
        .chain(model.threads.keys())
        .chain(cursors);
    let past = ids
        .copied()
        .find(|id| usize::try_from(*id).map_or(true, |i| i >= model.objects.len()));

    match past {
        Some(id) => Err(Refused::Missing(format!("object {id}"))),
        None => Ok(()),
    }
}
