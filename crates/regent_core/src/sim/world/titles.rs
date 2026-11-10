//! `ListTitles` and `RemoveTitles`: a list's agents by their GPL
//! `title`. **Binary** unless marked.

use super::{Answer, Attributes, TITLE, World, agent_of, list_of, string_ref};
use crate::gpl::ops::{Accessor, OpError, Value};
use crate::gpl::vm::host::NO_AGENT;
use crate::gpl::vm::{Fault, Handle, Vm};

impl World {
    /// `ListTitles(list, title)`, gpl list titles: each element whose
    /// [`TITLE`] attribute equals `title`, ignoring case (`_stricmp`),
    /// appended to the result in list order. An element's attribute is read
    /// as `'s` reads it: a missing agent throws [`NO_AGENT`].
    pub(super) fn titles(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
        gpl: &Attributes<'_>,
    ) -> std::result::Result<(), Fault> {
        let ret = vm.arg(args, 0)?;
        list_of(vm.object(ret))?;
        let list = vm.arg(args, 1)?;
        let items = list_of(vm.object(list))?.to_vec();
        let title = string_ref(vm, args, 2)?;
        let mut found = Vec::new();

        for item in items {
            let id = agent_of(&item)?;
            let agent = self
                .agents
                .get(id)
                .ok_or_else(|| Fault::throw(NO_AGENT.replacen("%s", TITLE, 1)))?;
            let current = match gpl(vm, id, TITLE).cloned() {
                Some(Value::String(s)) => Some(s),
                Some(_) => Err(OpError::BadConversion(Accessor::StringRef))?,
                None => agent.title.clone(),
            };

            if current.is_some_and(|t| t.eq_ignore_ascii_case(&title)) {
                found.push(Value::Agent(id));
            }
        }

        if let Value::List(out) = vm.object_mut(ret) {
            out.extend(found);
        }

        Ok(())
    }

    /// `RemoveTitles(list, title)`, gpl remove titles: each element of the list
    /// is taken off its front; one whose `title` equals `title` (`_stricmp`)
    /// goes to the result, any other back on the list's end. An element's
    /// `title` is read as `'s` reads it. Unknown for a list held in part
    /// (null elements).
    pub(super) fn remove_titles(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
        gpl: &Attributes<'_>,
    ) -> std::result::Result<Answer, Fault> {
        let ret = vm.arg(args, 0)?;
        list_of(vm.object(ret))?;
        let list = vm.arg(args, 1)?;
        let items = list_of(vm.object(list))?.to_vec();
        let title = string_ref(vm, args, 2)?;

        if items.contains(&Value::Null) {
            Ok(Answer::Unknown)
        } else {
            self.split_titles(vm, gpl, (ret, list), items, &title)
                .map(|()| Answer::Given)
        }
    }

    /// `RemoveTitles`' walk: matches to the result `ret`, the rest back to
    /// `list`.
    fn split_titles(
        &self,
        vm: &mut Vm<'_>,
        gpl: &Attributes<'_>,
        (ret, list): (crate::gpl::vm::ObjId, crate::gpl::vm::ObjId),
        items: Vec<Value>,
        title: &str,
    ) -> std::result::Result<(), Fault> {
        let mut kept = Vec::new();
        let mut found = Vec::new();

        for item in items {
            let id = agent_of(&item)?;
            let agent = self
                .agents
                .get(id)
                .ok_or_else(|| Fault::throw(super::NO_AGENT.replacen("%s", TITLE, 1)))?;
            let current = match gpl(vm, id, TITLE).cloned() {
                Some(Value::String(s)) => Some(s),
                Some(_) => Err(OpError::BadConversion(Accessor::StringRef))?,
                None => agent.title.clone(),
            };

            if current.is_some_and(|t| t.eq_ignore_ascii_case(title)) {
                found.push(item);
            } else {
                kept.push(item);
            }
        }

        *vm.object_mut(list) = Value::List(kept.into());

        if let Value::List(out) = vm.object_mut(ret) {
            out.extend(found);
        }

        Ok(())
    }
}
