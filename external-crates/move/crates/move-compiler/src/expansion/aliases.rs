// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::Diagnostic,
    expansion::alias_map_builder::*,
    ice,
    shared::{unique_map::UniqueMap, unique_set::UniqueSet, *},
};
use move_ir_types::location::{Loc, sp};
use move_symbol_pool::Symbol;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

//**************************************************************************************************
// Type Definitions
//**************************************************************************************************

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

//**************************************************************************************************
// Traits
//**************************************************************************************************

trait NamespaceEntry: Copy {
    fn namespace(m: &AliasMap) -> &UniqueMap<Name, Self>;
    fn namespace_mut(m: &mut AliasMap) -> &mut UniqueMap<Name, Self>;
    fn alias_entry(name: Name, entry: Self) -> AliasEntry;
    fn suggestion<F>(m: &AliasMap, name: Name, filter: F) -> Option<Name>
    where
        F: Fn(&Symbol, &Self) -> bool;

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
    ($ty:ty, $ty_param:pat, .$field:ident) => {
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

            fn suggestion<F>(m: &AliasMap, name: Name, filter: F) -> Option<Name>
            where
                F: Fn(&Symbol, &Self) -> bool,
            {
                let candidates = m
                    .$field
                    .iter()
                    .filter(|(_, name, value)| filter(name, value));
                suggest_levenshtein_candidate(
                    candidates,
                    name.value.as_str(),
                    |(_, candidate, _)| candidate.as_str(),
                )
                .map(|(loc, name, _)| sp(loc, *name))
            }
        }
    };
}

namespace_entry!(LeadingAccessEntry, LeadingAccessEntry::TypeParam, .leading_access);
namespace_entry!(MemberEntry, MemberEntry::TypeParam, .module_members);

//**************************************************************************************************
// Impls
//**************************************************************************************************

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

    pub fn resolve_member(&mut self, name: &Name) -> Option<(Name, MemberEntry)> {
        let (name, entry) = MemberEntry::find(self, name)?;
        match &entry {
            MemberEntry::Member(_, _, _) => Some((name, entry)),
            // Do not resolve to type parameters; they are kept as names during expansion.
            MemberEntry::TypeParam => None,
            // Do not resolve to lambeda parameters; they are kept as names during expansion and
            // handled during name resolution.
            MemberEntry::LambdaParam => None,
        }
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

    /// Determines if the provided name is a bound as a lambda parameter in the current or any
    /// previous scope.
    pub fn is_lambda_parameter(&self, name: &Name) -> bool {
        let mut current_scope = Some(self);
        loop {
            let Some(scope) = current_scope else {
                break false;
            };
            if let Some(entry) = scope.module_members.get(name) {
                return matches!(entry, MemberEntry::LambdaParam);
            }
            current_scope = scope.previous.as_deref();
        }
    }

    /// Finds a suggestion for a leading access name that is close to the given name.
    /// The filter function is used to restrict the kinds of entries considered, so that we can
    /// only suggest reasonable candidates (e.g., only suggest modules or addresses for friend
    /// statements).
    pub fn suggest_leading_access<F>(&self, filter_fn: F, name: &Name) -> Option<Name>
    where
        F: Fn(&Symbol, &LeadingAccessEntry) -> bool,
    {
        // Heuristic: We prefer closer-scope matches, even if they are not the closest in edit
        // distance. This means we search the current scope first, then the previous ones.

        // if this was actually a type parameter, we don't want to suggest anything.
        if let Some(LeadingAccessEntry::TypeParam) = self.leading_access.get(name) {
            return None;
        }

        LeadingAccessEntry::suggestion(self, *name, &filter_fn).or_else(|| {
            self.previous
                .as_ref()
                .and_then(|prev| prev.suggest_leading_access(filter_fn, name))
        })
    }

    /// Finds a suggestion for a member module that is close to the given name.
    /// The filter function is used to restrict the kinds of entries considered, so that we can
    /// only suggest reasonable candidates.
    pub fn suggest_module_member<F>(&self, filter_fn: F, name: &Name) -> Option<Name>
    where
        F: Fn(&Symbol, &MemberEntry) -> bool,
    {
        // Heuristic: We prefer closer-scope matches, even if they are not the closest in edit
        // distance. This means we search the current scope first, then the previous ones.

        // if this was actually a type parameter, we don't want to suggest anything.
        if let Some(MemberEntry::TypeParam) = self.module_members.get(name) {
            return None;
        }

        MemberEntry::suggestion(self, *name, &filter_fn).or_else(|| {
            self.previous
                .as_ref()
                .and_then(|prev| prev.suggest_module_member(filter_fn, name))
        })
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

    /// Similar to add_and_shadow but just hides aliases now shadowed by a lambda parameter.
    /// Lambda parameters are never resolved. We track them to apply appropriate shadowing to make
    /// suggetions better.
    pub fn push_lambda_parameters<'a, I: IntoIterator<Item = &'a Name>>(&mut self, lparams: I)
    where
        I::IntoIter: ExactSizeIterator,
    {
        let mut new_map = Self::new();
        for lparam in lparams {
            // ignore duplicates, they will be checked in naming
            let _ = new_map
                .module_members
                .add(*lparam, MemberEntry::LambdaParam);
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
                AliasEntry::Address(_, _)
                | AliasEntry::TypeParam(_)
                | AliasEntry::LambdaParam(_) => (),
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
