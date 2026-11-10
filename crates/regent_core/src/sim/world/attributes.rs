//! GPL attributes made, dropped or re-read at run time: `AddAttribute`,
//! `RemoveAttribute`, `UpgradeAgentAttributes`. **Binary** unless marked.
//!
//! ```text
//!  AddAttribute(agent, name, type, [value])
//!    no agent: true
//!    lacking it: the type's name, upper case, to a code:
//!                0 or 10 adds nothing; else a fresh value, `value`
//!                assigned, added; true
//!    holding it: `value` assigned to it; false
//!  RemoveAttribute(agent, name)
//!    the agent's attribute released and dropped
//!  UpgradeAgentAttributes(agent)
//!    unit or throw ── its descriptor's name as instance
//!    per prototype member: missing ─> added, a fresh value of its type
//!                          text not empty ─> set on the attribute
//!                          (a function: GplFunction, the function
//!                          only; a thread keeps interval, mode, frame)
//! ```
//!
//! The world keeps the names; the host owns the values and writes them
//! ([`World::take_reread`], [`World::take_removed`]).

use super::{
    Answer, Attributes, Found, Member, TITLE, World, agent_ref, from_text, put_bool, string_ref,
};
use crate::data::gpltype::Kind;
use crate::gpl::ops::{self, Value};
use crate::gpl::vm::{Fault, Handle, Vm};

/// Gpl type by name's names, by code: what `AddAttribute`'s third argument
/// may say, upper case. A name not here is code 0. **Inferred**: its
/// `"class Gpl…"` entries, mixed case, never match an upper case name.
pub const TYPE_NAMES: [(&str, u8); 10] = [
    ("UNKNOWTYPE", 10),
    ("INTEGER", 1),
    ("FLOAT", 2),
    ("STRING", 3),
    ("LIST", 4),
    ("AGENTREF", 5),
    ("BOOLEAN", 6),
    ("COORD", 7),
    ("FUNCTION", 8),
    ("NULLTYPE", 0),
];

/// `AddAttribute`'s optional value: argument 4.
const VALUE_ARG: usize = 4;

/// The class `AddAttribute`'s type name makes; `None` for codes 0 and 10.
fn class_named(name: &str) -> Option<Kind> {
    let upper = name.to_ascii_uppercase();

    TYPE_NAMES
        .iter()
        .find(|(n, _)| *n == upper)
        .and_then(|(_, code)| Kind::from_code(*code))
        .filter(|kind| *kind != Kind::Null)
}

impl World {
    /// `AddAttribute(agent, name, type, [value])`, gpl add attribute: true
    /// where the agent lacked the name. Unknown where it holds it, a
    /// value is given, and the host's value is not known.
    pub(super) fn add_attribute(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
        gpl: &Attributes<'_>,
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;
        let folded = name.to_ascii_lowercase();
        let value = if args.len() > VALUE_ARG {
            Some(vm.object(vm.arg(args, VALUE_ARG)?).clone())
        } else {
            None
        };
        let has = self.agents.get(id).map(|a| a.attributes.contains(&folded));

        // Lacking it: a fresh value of the type, the value assigned.
        let made = match has {
            Some(false) => {
                let kind = class_named(&string_ref(vm, args, 3)?);

                kind.map(|kind| {
                    let mut fresh = Value::fresh(kind);

                    value
                        .as_ref()
                        .map_or(Ok(()), |v| ops::assign(&mut fresh, v))
                        .map(|()| (kind, fresh))
                })
                .transpose()
                .map_err(Fault::from)?
            }
            _ => None,
        };

        // Holding it: the value assigned to the host's.
        let held = match (has, &value) {
            (Some(true), Some(v)) => match gpl(vm, id, &name).cloned() {
                Some(mut current) => {
                    ops::assign(&mut current, v).map_err(Fault::from)?;
                    Some(Some(current))
                }
                None => None,
            },
            _ => Some(None),
        };

        if let Some((kind, fresh)) = made {
            self.member(id, &name, kind, fresh, None);
        }

        if let Some(Some(current)) = &held {
            self.member(
                id,
                &name,
                current.kind(),
                current.clone(),
                Some(current.clone()),
            );
        }

        match held {
            Some(_) => {
                let ret = vm.arg(args, 0)?;

                put_bool(vm, ret, has != Some(true)).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// Attribute `name` of agent `id` for the host to make, or to set to
    /// `text`.
    fn member(&mut self, id: u32, name: &str, kind: Kind, value: Value, text: Option<Value>) {
        if let Some(agent) = self.agents.get_mut(id) {
            agent.attributes.insert(name.to_ascii_lowercase());
        }

        let fresh = Member {
            name: name.to_owned(),
            ty: kind.code(),
            value,
        };

        self.agents.reread(id, vec![Reread { fresh, text }]);
    }

    /// `RemoveAttribute(agent, name)`, gpl remove attribute: with an agent, its
    /// attribute released and dropped; none, nothing. **Inferred**: a
    /// thread on a dropped function runs on; the SDK kills it first.
    pub(super) fn remove_attribute(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?.to_ascii_lowercase();

        if let Some(agent) = self.agents.get_mut(id)
            && agent.attributes.remove(&name)
        {
            self.agents.removed(id, name);
        }

        Ok(Answer::Given)
    }

    /// The attributes `RemoveAttribute` dropped since the last call, by
    /// agent and folded name, for the host to drop.
    pub fn take_removed(&mut self) -> Vec<(u32, String)> {
        self.agents.take_removals()
    }

    /// The agents `UnitDelete` ended since the last call, in order: whose
    /// attribute objects the host frees.
    pub fn take_deleted(&mut self) -> Vec<u32> {
        self.agents.take_deleted()
    }
}

/// One member as the re-read sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reread {
    /// The member as a fresh attribute would be made: [`from_text`] of
    /// the empty text (**Inferred**: gpl value of type's value).
    pub fresh: Member,
    /// The instance's value, where its text is not empty.
    pub text: Option<crate::gpl::ops::Value>,
}

impl World {
    /// `UpgradeAgentAttributes(agent)`: throws with no unit; else the
    /// members of the unit's instance, for the host.
    pub(super) fn upgrade_attributes(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = match self.found(id) {
            Found::Piece(p) => Some(p.name.clone()),
            Found::Gone => Err(Fault::throw("UpgradeAgentAttributes() unit is null"))?,
            Found::Unsure => None,
        };

        match name {
            Some(name) => {
                let members = reread(vm, &name)?;

                if let Some(agent) = self.agents.get_mut(id) {
                    for m in &members {
                        agent.attributes.insert(m.fresh.name.to_ascii_lowercase());
                    }

                    let title = members
                        .iter()
                        .find(|m| m.fresh.name.eq_ignore_ascii_case(TITLE))
                        .and_then(|m| m.text.clone());

                    if let Some(crate::gpl::ops::Value::String(t)) = title {
                        agent.title = Some(t);
                    }
                }

                self.agents.reread(id, members);
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// The re-reads made since the last call, for the host to write.
    pub fn take_reread(&mut self) -> Vec<(u32, Vec<Reread>)> {
        self.agents.take_rereads()
    }
}

/// Instance `name`'s members, prototype order: gpl init data find,
/// then gpl prototype data find, each member's text by
/// gpl init data text.
fn reread(vm: &Vm<'_>, name: &str) -> std::result::Result<Vec<Reread>, Fault> {
    let program = vm.program();
    let set = program
        .data_set(name)
        .ok_or_else(|| Fault::throw("couldn't find initialization data"))?;
    let members = program
        .prototype(program.names().get(set.shape))
        .ok_or_else(|| Fault::throw("couldn't find prototype data"))?;

    members
        .iter()
        .map(|m| {
            let member = program.names().get(m.name);
            let text = set
                .values
                .iter()
                .find(|(k, _)| program.names().get(*k).eq_ignore_ascii_case(member))
                .map_or("", |(_, v)| program.names().get(*v));
            let value = match text {
                "" => None,
                t => Some(from_text(program, m.ty.code(), t)?),
            };

            Ok(Reread {
                fresh: Member {
                    name: member.to_owned(),
                    ty: m.ty.code(),
                    value: from_text(program, m.ty.code(), "")?,
                },
                text: value,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::Agent;

    /// The SDK's three types read; a name gpl type by name lacks, as
    /// `"agent"`, or `"unknowtype"`, adds nothing.
    #[test]
    fn a_type_name_is_read_in_upper_case() {
        assert_eq!(class_named("boolean"), Some(Kind::Boolean));
        assert_eq!(class_named("integer"), Some(Kind::Integer));
        assert_eq!(class_named("function"), Some(Kind::Function));
        assert_eq!(class_named("AgentRef"), Some(Kind::Agent));
        assert_eq!(class_named("agent"), None);
        assert_eq!(class_named("unknowtype"), None);
        assert_eq!(class_named("nulltype"), None);
    }

    /// `SpecialItemExample`'s `FoundMagicBall`: added once, true; again,
    /// false; removed, the host told.
    #[test]
    fn an_added_attribute_is_named_and_handed_to_the_host() {
        let program = crate::gpl::load::Program::load::<&[u8]>(&[]).expect("empty program");
        let mut vm = Vm::new(&program, crate::gpl::vm::Config::default());
        let mut world = World::default();
        let args: Vec<Handle> = [
            Value::Boolean(false),
            Value::Agent(1),
            Value::String("FoundMagicBall".into()),
            Value::String("boolean".into()),
            Value::Boolean(true),
        ]
        .into_iter()
        .map(|v| {
            let o = vm.alloc(v);

            vm.top_handle(Some(o))
        })
        .collect();
        let yes: &'static Value = Box::leak(Box::new(Value::Boolean(true)));
        let none: &Attributes<'_> = &|_, _, _| None;
        let held: &Attributes<'_> = &|_, _, _| Some(yes);

        world.agents.insert(1, Agent::default());

        assert_eq!(
            world.add_attribute(&mut vm, &args, &none),
            Ok(Answer::Given)
        );
        assert_eq!(
            vm.object(vm.arg(&args, 0).expect("result")),
            &Value::Boolean(true)
        );
        assert!(
            world
                .agents
                .get(1)
                .expect("agent 1")
                .attributes
                .contains("foundmagicball")
        );

        let made = world.take_reread();

        assert_eq!(made.len(), 1);
        assert_eq!(made[0].1[0].fresh.value, Value::Boolean(true));
        assert_eq!(made[0].1[0].text, None);

        assert_eq!(
            world.add_attribute(&mut vm, &args, &none),
            Ok(Answer::Unknown)
        );
        assert_eq!(
            world.add_attribute(&mut vm, &args, &held),
            Ok(Answer::Given)
        );
        assert_eq!(
            vm.object(vm.arg(&args, 0).expect("result")),
            &Value::Boolean(false)
        );

        assert_eq!(
            world.remove_attribute(&mut vm, &args[..3]),
            Ok(Answer::Given)
        );
        assert_eq!(world.take_removed(), [(1, "foundmagicball".to_owned())]);
        assert!(world.agents.get(1).expect("agent 1").attributes.is_empty());
    }
}
