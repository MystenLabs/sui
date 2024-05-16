// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_ir_types::location::Loc;

use crate::{
    diagnostics::Diagnostic,
    ice,
    naming::alias_map_builder::*,
    shared::{unique_map::UniqueMap, unique_set::UniqueSet, *},
};
use std::{collections::BTreeSet, fmt};

use super::{
    ast::{BuiltinFunction_, BuiltinTypeName_},
    name_resolver::ResolvedDefinition,
};

#[derive(Clone, Debug)]
pub struct NameSet {
    pub modules: UniqueSet<Name>,
    pub members: UniqueSet<Name>,
    pub kind: NameMapKind,
}

#[derive(Clone, Copy, Debug)]
pub enum NameMapKind {
    TypeParameters,
    Use,
    Addresses,
    Builtins,
    LegacyTopLevel,
}

pub struct NameMap {
    unused: BTreeSet<AliasEntry>,
    // the start of an access path, excludes functions
    leading_access: UniqueMap<Name, LeadingAccessEntry>,
    // a module member is expected, not a module
    // For now, this excludes local variables because the only case where this can overlap is with
    // macro lambdas, but those have to have a leading `$` and cannot conflict with module members
    module_members: UniqueMap<Name, ResolvedDefinition>,
    pub kind: NameMapKind,
    previous: Option<Box<NameMap>>,
}

trait NamespaceEntry: Copy {
    fn namespace(m: &NameMap) -> &UniqueMap<Name, Self>;
    fn namespace_mut(m: &mut NameMap) -> &mut UniqueMap<Name, Self>;
    fn alias_entry(name: Name, entry: Self) -> AliasEntry;

    fn find_custom(
        m: &mut NameMap,
        name: &Name,
        mut f_entry: impl FnMut(&mut NameMap, &Name, &Self),
    ) -> Option<(Name, Self)> {
        let mut current_scope = Some(m);
        loop {
            let Some(scope) = current_scope else {
                break None;
            };
            let Some(entry) = Self::namespace(scope).get(name).copied() else {
                current_scope = scope.previous.as_deref_mut();
                continue;
            };
            let original_name = Self::namespace_mut(scope).get_full_key(name).unwrap();
            f_entry(scope, &original_name, &entry);
            break Some((original_name, entry));
        }
    }

    fn find(m: &mut NameMap, name: &Name) -> Option<(Name, Self)> {
        Self::find_custom(m, name, |scope, name, entry| {
            scope.unused.remove(&Self::alias_entry(*name, *entry));
        })
    }
}

macro_rules! namespace_entry {
    ($ty:ty, .$field:ident) => {
        impl NamespaceEntry for $ty {
            fn namespace(m: &NameMap) -> &UniqueMap<Name, Self> {
                &m.$field
            }

            fn namespace_mut(m: &mut NameMap) -> &mut UniqueMap<Name, Self> {
                &mut m.$field
            }

            fn alias_entry(name: Name, entry: Self) -> AliasEntry {
                (name, entry).into()
            }
        }
    };
}

namespace_entry!(LeadingAccessEntry, .leading_access);
namespace_entry!(ResolvedDefinition, .module_members);

impl NameSet {
    pub fn new(kind: NameMapKind) -> Self {
        Self {
            modules: UniqueSet::new(),
            members: UniqueSet::new(),
            kind,
        }
    }

    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty() && self.members.is_empty()
    }
}

impl NameMap {
    /// Create a new NameMap. Note that it comes auto-propagated with all bultin types and
    /// functions.
    pub fn new() -> Self {
        let mut leading_access_types = UniqueMap::new();
        let mut module_members = UniqueMap::new();
        for (name, defn) in BuiltinTypeName_::all_types() {
            leading_access_types.add(name, ResolvedDefinition::BuiltinType(defn));
            module_members.add(name, ResolvedDefinition::BuiltinType(defn));
        }
        module_members.add(
            BuiltinFunction_::ASSERT_MACRO,
            ResolvedDefinition::BuiltinFun(BuiltinFunction_::Assert(None)),
        );
        module_members.add(
            BuiltinFunction_::FREEZE,
            ResolvedDefinition::BuiltinFun(BuiltinFunction_::Freeze(None)),
        );
        Self {
            unused: BTreeSet::new(),
            leading_access: UniqueMap::new(),
            module_members: UniqueMap::new(),
            kind: NameMapKind::Builtins,
            previous: None,
        }
    }

    pub fn resolve_leading_access(&mut self, name: &Name) -> Option<(Name, LeadingAccessEntry)> {
        LeadingAccessEntry::find(self, name)
    }

    pub fn resolve_member(&mut self, name: &Name) -> Option<(Name, ResolvedDefinition)> {
        ResolvedDefinition::find(self, name)
    }

    pub fn resolve(&mut self, namespace: NameSpace, name: &Name) -> Option<AliasEntry> {
        match namespace {
            NameSpace::LeadingAccess => self
                .resolve_leading_access(name)
                .map(|resolved| resolved.into()),
            NameSpace::ModuleMembers => self.resolve_member(name).map(|resolved| resolved.into()),
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
    /// Returns any name collisions that occur between addresses, members, modules, and type
    /// parameters in the map builder.
    pub fn push_alias_scope(
        &mut self,
        loc: Loc,
        new_aliases: AliasMapBuilder,
    ) -> Result<Vec<UnnecessaryAlias>, Box<Diagnostic>> {
        let AliasMapBuilder::Namespaced {
            leading_access: new_leading_access,
            module_members: new_module_members,
            kind,
        } = new_aliases
        else {
            return Err(Box::new(ice!((
                loc,
                "ICE alias map builder should be namespaced for 2024 paths"
            ))));
        };

        let mut unused = BTreeSet::new();
        let mut duplicate = vec![];
        for (alias, (entry, is_implicit)) in new_leading_access.key_cloned_iter() {
            if !*is_implicit {
                unused.insert((alias, *entry).into());
                LeadingAccessEntry::find_custom(self, &alias, |scope, prev_name, prev_entry| {
                    if entry == prev_entry {
                        duplicate.push(UnnecessaryAlias {
                            entry: (alias, *entry).into(),
                            prev: prev_name.loc,
                        });
                        scope.unused.remove(&(*prev_name, *prev_entry).into());
                    }
                });
            }
        }
        for (alias, (entry, is_implicit)) in new_module_members.key_cloned_iter() {
            if !*is_implicit {
                unused.insert((alias, *entry).into());
                ResolvedDefinition::find_custom(self, &alias, |scope, prev_name, prev_entry| {
                    if entry == prev_entry {
                        duplicate.push(UnnecessaryAlias {
                            entry: (alias, *entry).into(),
                            prev: prev_name.loc,
                        });
                        scope.unused.remove(&(*prev_name, *prev_entry).into());
                    }
                });
            }
        }

        let leading_access = new_leading_access.map(|_alias, (entry, _is_implicit)| entry);
        let module_members = new_module_members.map(|_alias, (entry, _is_implicit)| entry);

        let new_map = Self {
            unused,
            kind,
            leading_access,
            module_members,
            previous: None,
        };

        // set the previous scope
        let previous = std::mem::replace(self, new_map);
        self.previous = Some(Box::new(previous));
        Ok(duplicate)
    }

    /// Resets the alias map to the previous scope, and returns the set of unused aliases
    pub fn pop_scope(&mut self) -> NameSet {
        let previous = self
            .previous
            .take()
            .map(|prev| *prev)
            .unwrap_or_else(Self::new);
        let popped = std::mem::replace(self, previous);
        let mut result = NameSet::new(popped.kind);
        for alias_entry in popped.unused {
            match alias_entry {
                AliasEntry::Module(name, _) => result.modules.add(name).unwrap(),
                AliasEntry::Definition(name, _) => result.members.add(name).unwrap(),
                AliasEntry::Address(_, _) => (),
            }
        }
        result
    }
}

//**************************************************************************************************
// Debug
//**************************************************************************************************

impl fmt::Debug for NameMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        let Self {
            unused,
            kind,
            leading_access,
            module_members,
            previous,
        } = self;
        writeln!(f, "AliasMap({kind:?}\n  unused: [")?;
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
