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
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

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
    // These are for caching resolution for IDE information.
    ide_alias_info: Option<ide::AliasAutocompleteInfo>,
    previous: Option<Box<AliasMap>>,
}

trait NamespaceEntry: Copy {
    fn namespace(m: &AliasMap) -> &UniqueMap<Name, Self>;
    fn namespace_mut(m: &mut AliasMap) -> &mut UniqueMap<Name, Self>;
    fn alias_entry(name: Name, entry: Self) -> AliasEntry;

    fn find_custom(
        m: &mut AliasMap,
        name: &Name,
        mut f_entry: impl FnMut(&mut AliasMap, &Name, &Self),
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

    fn find(m: &mut AliasMap, name: &Name) -> Option<(Name, Self)> {
        Self::find_custom(m, name, |scope, name, entry| {
            scope.unused.remove(&Self::alias_entry(*name, *entry));
        })
    }
}

macro_rules! namespace_entry {
    ($ty:ty, .$field:ident) => {
        impl NamespaceEntry for $ty {
            fn namespace(m: &AliasMap) -> &UniqueMap<Name, Self> {
                &m.$field
            }

            fn namespace_mut(m: &mut AliasMap) -> &mut UniqueMap<Name, Self> {
                &mut m.$field
            }

            fn alias_entry(name: Name, entry: Self) -> AliasEntry {
                (name, entry).into()
            }
        }
    };
}

namespace_entry!(LeadingAccessEntry, .leading_access);
namespace_entry!(MemberEntry, .module_members);

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

impl AliasMap {
    pub fn new() -> Self {
        Self {
            unused: BTreeSet::new(),
            leading_access: UniqueMap::new(),
            module_members: UniqueMap::new(),
            ide_alias_info: None,
            previous: None,
        }
    }

    pub fn resolve_leading_access(&mut self, name: &Name) -> Option<(Name, LeadingAccessEntry)> {
        let (name, entry) = LeadingAccessEntry::find(self, name)?;
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
        let (name, entry) = MemberEntry::find(self, name)?;
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
    ) -> Result<Vec<UnnecessaryAlias>, Box<Diagnostic>> {
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
                MemberEntry::find_custom(self, &alias, |scope, prev_name, prev_entry| {
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
            leading_access,
            module_members,
            ide_alias_info: None,
            previous: None,
        };

        // set the previous scope
        let previous = std::mem::replace(self, new_map);
        self.previous = Some(Box::new(previous));
        Ok(duplicate)
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

    /// Gets a map of all in-scope names for IDE information, subject to shadowing, either from a
    /// cached value or generated fresh.
    pub fn get_ide_alias_information(&mut self) -> ide::AliasAutocompleteInfo {
        if self.ide_alias_info.is_none() {
            let mut cur: Option<&Self> = Some(self);
            let mut leading_names = BTreeMap::new();
            let mut member_names = BTreeMap::new();
            while let Some(map) = cur {
                for (name, entry) in map.leading_access.key_cloned_iter() {
                    leading_names.entry(name.value).or_insert(*entry);
                }
                for (name, entry) in map.module_members.key_cloned_iter() {
                    member_names.entry(name.value).or_insert(*entry);
                }
                cur = map.previous.as_deref();
            }
            self.ide_alias_info = Some((leading_names, member_names).into())
        }
        self.ide_alias_info.clone().unwrap()
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
            ide_alias_info: _,
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
