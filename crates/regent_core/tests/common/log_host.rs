//! A host with no world, for VM tests.

use regent_core::base::digest::Map;
use regent_core::data::gpltype::TypeCode;
use regent_core::gpl::exec::Kind;
use regent_core::gpl::load::Program;
use regent_core::gpl::ops::{NULL_AGENT, Value};
use regent_core::gpl::vm::host::NO_AGENT;
use regent_core::gpl::vm::{Call, Fault, Host, ObjId, SHARED_NULL, Vm};

/// A builtin call as [`LogHost`] saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Logged {
    pub name: &'static str,
    /// Arguments 1 on; `None` for an empty handle.
    pub args: Vec<Option<Value>>,
}

/// A host with no world: records each call and leaves the return slot at the
/// registered type's default; a `#Name` is the integer 0.
///
/// Every agent but the null one exists and has every attribute some
/// prototype declares, of the first declared type, made on first read and
/// kept; any other name is the shared null. The null agent throws
/// [`NO_AGENT`], as the engine does.
#[derive(Clone, Debug, Default)]
pub struct LogHost {
    pub calls: Vec<Logged>,
    /// Every attribute read: agent and name.
    pub attributes: Vec<(u32, String)>,
    pub log: Vec<String>,
    /// Attribute types by folded name, from the prototypes.
    types: Map<String, TypeCode>,
    made: Map<(u32, String), ObjId>,
}

impl LogHost {
    /// A host whose agents carry `program`'s prototype attributes.
    #[must_use]
    pub fn new(program: &Program) -> LogHost {
        let mut types = Map::default();

        for definition in program.definitions() {
            if let Kind::Prototype(members) = &definition.kind {
                for m in members {
                    let name = program.names().get(m.name).to_ascii_lowercase();
                    types.entry(name).or_insert(m.ty);
                }
            }
        }

        LogHost {
            types,
            ..LogHost::default()
        }
    }
}

impl Host for LogHost {
    fn builtin(&mut self, vm: &mut Vm<'_>, call: &Call<'_>) -> Result<(), Fault> {
        let args = call
            .args
            .iter()
            .skip(1)
            .map(|h| vm.value(*h).cloned())
            .collect();

        self.calls.push(Logged {
            name: call.builtin().name,
            args,
        });

        Ok(())
    }

    fn attribute(&mut self, vm: &mut Vm<'_>, agent: u32, name: &str) -> Result<ObjId, Fault> {
        let key = (agent, name.to_ascii_lowercase());

        self.attributes.push((agent, name.to_owned()));

        if agent == NULL_AGENT {
            Err(Fault::throw(NO_AGENT.replacen("%s", name, 1)))
        } else if let Some(o) = self.made.get(&key) {
            Ok(*o)
        } else {
            let o = self
                .types
                .get(&key.1)
                .and_then(|ty| vm.of_type(*ty))
                .unwrap_or(SHARED_NULL);

            self.made.insert(key, o);
            Ok(o)
        }
    }

    fn named(&mut self, _name: &str) -> Value {
        Value::Integer(0)
    }

    fn log(&mut self, text: &str) {
        self.log.push(text.to_owned());
    }
}
