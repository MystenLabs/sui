// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_ir_types::location::Loc;

use crate::{
    diagnostics::Diagnostic,
    expansion::alias_map_builder::*,
    ice,
    shared::{unique_map::UniqueMap, unique_set::UniqueSet, *},
};
use std::{collections::BTreeSet, fmt};

#[derive(Clone, Debug)]
pub struct AliasSet {
    pub modules: UniqueSet<Name>,
    pub members: UniqueSet<Name>,
}

pub struct AliasMap {
    unused: BTreeSet<AliasEntry>,
    // the start of an access path, excludes functions
    leading_access: UniqueMap<Name, LeadingAccessEntry>,
    // a module member is expected, not a module
    // For now, this excludes local variables because the only case where this can overlap is with
    // macro lambdas, but those have to have a leading `$` and cannot conflict with module members
    module_members: UniqueMap<Name, MemberEntry>,
    previous: Option<Box<AliasMap>>,
}

impl AliasSet {
    pub fn new() -> Self {
        Self {
            modules: UniqueSet::new(),
            members: UniqueSet::new(),
        }
    }

    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty() && self.members.is_empty()
    }
}

macro_rules! resolve_alias {
    ($map:expr, .$namespace:ident, $name:expr) => {{
        let mut current_scope = Some($map);
        let name = $name;
        loop {
            let Some(scope) = current_scope else {
                break None;
            };
            let Some(entry) = scope.$namespace.get(name).copied() else {
                current_scope = scope.previous.as_mut().map(|x| &mut **x);
                continue;
            };
            // Note, might have already been removed by a different `$namespace` resolution
            scope.unused.remove(&(*name, entry).into());

            let original_name = scope.$namespace.get_full_key(&name).unwrap();
            break Some((original_name, entry));
        }
    }};
}

impl AliasMap {
    pub fn new() -> Self {
        Self {
            unused: BTreeSet::new(),
            leading_access: UniqueMap::new(),
            module_members: UniqueMap::new(),
            previous: None,
        }
    }

    pub fn resolve_leading_access(&mut self, name: &Name) -> Option<(Name, LeadingAccessEntry)> {
        let (name, entry) = resolve_alias!(self, .leading_access, name)?;
        match &entry {
            LeadingAccessEntry::Module(_)
            | LeadingAccessEntry::Address(_)
            | LeadingAccessEntry::Member(_, _) => Some((name, entry)),
            // For code legacy reasons, don't resolve type parameters, they are just here for
            // shadowing
            LeadingAccessEntry::TypeParam => None,
        }
    }

    pub fn resolve_call(&mut self, name: &Name) -> Option<(Name, MemberEntry)> {
        let (name, entry) = resolve_alias!(self, .module_members, name)?;
        match &entry {
            MemberEntry::Member(_, _) => Some((name, entry)),
            // For code legacy reasons, don't resolve type parameters, they are just here for
            // shadowing
            MemberEntry::TypeParam => None,
        }
    }

    pub fn resolve(&mut self, namespace: NameSpace, name: &Name) -> Option<AliasEntry> {
        match namespace {
            NameSpace::LeadingAccess => self
                .resolve_leading_access(name)
                .map(|resolved| resolved.into()),
            NameSpace::ModuleMembers => self.resolve_call(name).map(|resolved| resolved.into()),
        }
    }

    pub fn resolve_any_for_error(&mut self, name: &Name) -> Option<AliasEntry> {
        for namespace in [NameSpace::LeadingAccess, NameSpace::ModuleMembers] {
            if let Some(entry) = self.resolve(namespace, name) {
                return Some(entry);
            }
        }
        None
    }

    /// Pushes a new scope, adding all of the new items to it (shadowing the outer one).
    /// Returns any name collisions that occur between addresses, members, and modules in the map
    /// builder.
    pub fn push_alias_scope(
        &mut self,
        loc: Loc,
        new_aliases: AliasMapBuilder,
    ) -> Result<(), Box<Diagnostic>> {
        let AliasMapBuilder::Namespaced {
            leading_access: new_leading_access,
            module_members: new_module_members,
        } = new_aliases
        else {
            return Err(Box::new(ice!((
                loc,
                "ICE alias map builder should be namespaced for 2024 paths"
            ))));
        };

        let mut unused = BTreeSet::new();
        for (alias, (entry, is_implicit)) in new_leading_access.key_cloned_iter() {
            if !*is_implicit {
                unused.insert((alias, *entry).into());
            }
        }
        for (alias, (entry, is_implicit)) in new_module_members.key_cloned_iter() {
            if !*is_implicit {
                unused.insert((alias, *entry).into());
            }
        }

        let leading_access = new_leading_access.map(|_alias, (entry, _is_implicit)| entry);
        let module_members = new_module_members.map(|_alias, (entry, _is_implicit)| entry);

        let new_map = Self {
            unused,
            leading_access,
            module_members,
            previous: None,
        };

        // set the previous scope
        let previous = std::mem::replace(self, new_map);
        self.previous = Some(Box::new(previous));
        Ok(())
    }

    /// Similar to add_and_shadow but just hides aliases now shadowed by a type parameter.
    /// Type parameters are never resolved. We track them to apply appropriate shadowing.
    pub fn push_type_parameters<'a, I: IntoIterator<Item = &'a Name>>(&mut self, tparams: I)
    where
        I::IntoIter: ExactSizeIterator,
    {
        let mut new_map = Self::new();
        for tparam in tparams {
            // ignore duplicates, they will be checked in naming
            let _ = new_map
                .leading_access
                .add(*tparam, LeadingAccessEntry::TypeParam);
            let _ = new_map.module_members.add(*tparam, MemberEntry::TypeParam);
        }

        // set the previous scope
        let previous = std::mem::replace(self, new_map);
        self.previous = Some(Box::new(previous));
    }

    /// Resets the alias map to the previous scope, and returns the set of unused aliases
    pub fn pop_scope(&mut self) -> AliasSet {
        let previous = self
            .previous
            .take()
            .map(|prev| *prev)
            .unwrap_or_else(Self::new);
        let popped = std::mem::replace(self, previous);
        let mut result = AliasSet::new();
        for alias_entry in popped.unused {
            match alias_entry {
                AliasEntry::Module(name, _) => result.modules.add(name).unwrap(),
                AliasEntry::Member(name, _, _) => result.members.add(name).unwrap(),
                AliasEntry::Address(_, _) | AliasEntry::TypeParam(_) => (),
            }
        }
        result
    }
}

//**************************************************************************************************
// Debug
//**************************************************************************************************

impl fmt::Debug for AliasMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        let Self {
            unused,
            leading_access,
            module_members,
            previous,
        } = self;
        writeln!(f, "AliasMap(\n  unused: [")?;
        for entry in unused {
            writeln!(f, "    {entry:?},")?;
        }
        writeln!(f, "],\n  modules: [")?;
        for (_, alias, entry) in leading_access {
            writeln!(f, "    {alias} => {entry:?}")?;
        }
        writeln!(f, "],\n  members: [")?;
        for (_, alias, entry) in module_members {
            writeln!(f, "    {alias} => {entry:?}")?;
        }
        writeln!(f, "])")?;
        writeln!(f, "--> PREVIOUS \n: {previous:?}")
    }
}
