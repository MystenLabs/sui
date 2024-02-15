// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    expansion::ast::{self as E, ModuleIdent},
    expansion::translate::ModuleMemberKind,
    parser::ast::{self as P},
    shared::{unique_map::UniqueMap, unique_set::UniqueSet, *},
};
use move_ir_types::location::*;
use std::{collections::BTreeSet, fmt};

#[derive(Clone, Debug)]
pub struct AliasSet {
    pub modules: UniqueSet<Name>,
    pub members: UniqueSet<Name>,
}

pub type MemberName = (ModuleIdent, Name, ModuleMemberKind);

#[derive(Clone)]
pub enum AliasMapBuilder {
    Legacy {
        modules: UniqueMap<Name, (ModuleIdent, /* is_implicit */ bool)>,
        members: UniqueMap<Name, (MemberName, /* is_implicit */ bool)>,
    },
    Namespaced {
        leading_access: UniqueMap<Name, (LeadingAccessEntry, /* is_implicit */ bool)>,
        module_members: UniqueMap<Name, (MemberEntry, /* is_implicit */ bool)>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AliasEntry {
    Address(Name, NumericalAddress),
    Module(Name, ModuleIdent),
    Member(Name, ModuleIdent, Name),
    TypeParam(Name),
}

#[derive(Clone, Copy)]
pub enum LeadingAccessEntry {
    Address(NumericalAddress),
    Module(ModuleIdent),
    Member(ModuleIdent, Name),
    TypeParam,
}

#[derive(Clone, Copy)]
pub enum MemberEntry {
    Member(ModuleIdent, Name),
    TypeParam,
}

#[derive(Clone, Copy)]
pub enum NameSpace {
    LeadingAccess,
    ModuleMembers,
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

pub struct ParserExplicitUseFun {
    pub loc: Loc,
    pub attributes: E::Attributes,
    pub is_public: Option<Loc>,
    pub function: Box<P::NameAccessChain>,
    pub ty: Box<P::NameAccessChain>,
    pub method: Name,
}

pub struct UseFunsBuilder {
    pub explicit: Vec<ParserExplicitUseFun>,
    pub implicit: UniqueMap<Name, E::ImplicitUseFunCandidate>,
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

/// Remove a duplicate element in the map, returning its location as an error if it exists
fn remove_dup<K: TName, V>(map: &mut UniqueMap<K, V>, alias: &K) -> Result<(), K::Loc> {
    let loc = map.get_loc(alias).copied();
    match map.remove(alias) {
        None => Ok(()),
        Some(_) => Err(loc.unwrap()),
    }
}

impl AliasMapBuilder {
    /// Create a new AliasMapBuilder for legacy
    pub fn legacy() -> Self {
        Self::Legacy {
            modules: UniqueMap::new(),
            members: UniqueMap::new(),
        }
    }

    pub fn namespaced() -> Self {
        Self::Namespaced {
            leading_access: UniqueMap::new(),
            module_members: UniqueMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Legacy { modules, members } => modules.is_empty() && members.is_empty(),
            Self::Namespaced {
                leading_access,
                module_members,
            } => leading_access.is_empty() && module_members.is_empty(),
        }
    }

    fn remove_module_alias(&mut self, alias: &Name) -> Result<(), Loc> {
        match self {
            Self::Legacy { modules, .. } => remove_dup(modules, alias),
            Self::Namespaced { leading_access, .. } => remove_dup(leading_access, alias),
        }
    }

    fn remove_member_alias(&mut self, alias: &Name, kind: ModuleMemberKind) -> Result<(), Loc> {
        match (self, kind) {
            (AliasMapBuilder::Legacy { members, .. }, _) => remove_dup(members, alias),
            // constants and functions are not in the leading access namespace
            (
                AliasMapBuilder::Namespaced {
                    leading_access: _,
                    module_members,
                },
                ModuleMemberKind::Constant | ModuleMemberKind::Function | ModuleMemberKind::Schema,
            ) => remove_dup(module_members, alias),
            // structs are in the leading access namespace in addition to the module members
            // namespace
            (
                AliasMapBuilder::Namespaced {
                    leading_access,
                    module_members,
                },
                ModuleMemberKind::Struct,
            ) => {
                let r1 = remove_dup(module_members, alias);
                let r2 = remove_dup(leading_access, alias);
                r1.and(r2)
            }
        }
    }

    fn remove_address_alias(&mut self, alias: &Name) -> Result<(), Loc> {
        match self {
            Self::Legacy { .. } => Ok(()),
            Self::Namespaced { leading_access, .. } => remove_dup(leading_access, alias),
        }
    }

    /// Adds a module alias to the map.
    /// Errors if one already bound for that alias
    fn add_module_alias_(
        &mut self,
        alias: Name,
        ident: ModuleIdent,
        is_implicit: bool,
    ) -> Result<(), Loc> {
        let result = self.remove_module_alias(&alias);
        match self {
            Self::Legacy { modules, .. } => modules.add(alias, (ident, is_implicit)).unwrap(),
            Self::Namespaced { leading_access, .. } => {
                let entry = (LeadingAccessEntry::Module(ident), is_implicit);
                leading_access.add(alias, entry).unwrap()
            }
        }
        result
    }

    fn add_member_alias_(
        &mut self,
        alias: Name,
        ident: ModuleIdent,
        member: Name,
        kind: ModuleMemberKind,
        is_implicit: bool,
    ) -> Result<(), Loc> {
        let result = self.remove_member_alias(&alias, kind);
        match (self, kind) {
            (AliasMapBuilder::Legacy { members, .. }, _) => members
                .add(alias, ((ident, member, kind), is_implicit))
                .unwrap(),
            // constants and functions are not in the leading access namespace
            (
                AliasMapBuilder::Namespaced {
                    leading_access: _,
                    module_members,
                },
                ModuleMemberKind::Constant | ModuleMemberKind::Function | ModuleMemberKind::Schema,
            ) => {
                let entry = (MemberEntry::Member(ident, member), is_implicit);
                module_members.add(alias, entry).unwrap();
            }
            // structs are in the leading access namespace in addition to the module members
            // namespace
            (
                AliasMapBuilder::Namespaced {
                    leading_access,
                    module_members,
                },
                ModuleMemberKind::Struct,
            ) => {
                let member_entry = (MemberEntry::Member(ident, member), is_implicit);
                module_members.add(alias, member_entry).unwrap();
                let leading_access_entry = (LeadingAccessEntry::Member(ident, member), is_implicit);
                leading_access.add(alias, leading_access_entry).unwrap();
            }
        }
        result
    }

    fn add_address_alias_(
        &mut self,
        alias: Name,
        address: NumericalAddress,
        is_implicit: bool,
    ) -> Result<(), Loc> {
        let result = self.remove_address_alias(&alias);
        match self {
            Self::Legacy { .. } => (),
            Self::Namespaced { leading_access, .. } => {
                let entry = (LeadingAccessEntry::Address(address), is_implicit);
                leading_access.add(alias, entry).unwrap()
            }
        }
        result
    }

    /// Adds a module alias to the map.
    /// Errors if one already bound for that alias
    pub fn add_module_alias(&mut self, alias: Name, ident: ModuleIdent) -> Result<(), Loc> {
        self.add_module_alias_(alias, ident, /* is_implicit */ false)
    }

    /// Adds a member alias to the map.
    /// Errors if one already bound for that alias
    pub fn add_member_alias(
        &mut self,
        alias: Name,
        ident: ModuleIdent,
        member: Name,
        kind: ModuleMemberKind,
    ) -> Result<(), Loc> {
        self.add_member_alias_(alias, ident, member, kind, /* is_implicit */ false)
    }

    /// Adds an address alias to the map.
    /// Errors if one already bound for that alias
    pub fn add_address_alias(&mut self, alias: Name, address: NumericalAddress) -> Result<(), Loc> {
        self.add_address_alias_(alias, address, /* is_implicit */ false)
    }

    /// Same as `add_module_alias` but it does not update the scope, and as such it will not be
    /// reported as unused
    pub fn add_implicit_module_alias(
        &mut self,
        alias: Name,
        ident: ModuleIdent,
    ) -> Result<(), Loc> {
        self.add_module_alias_(alias, ident, /* is_implicit */ true)
    }

    /// Same as `add_member_alias` but it does not update the scope, and as such it will not be
    /// reported as unused
    pub fn add_implicit_member_alias(
        &mut self,
        alias: Name,
        ident: ModuleIdent,
        member: Name,
        kind: ModuleMemberKind,
    ) -> Result<(), Loc> {
        self.add_member_alias_(alias, ident, member, kind, /* is_implicit */ true)
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
            // For legacy reasons, don't resolve type parameters
            LeadingAccessEntry::TypeParam => None,
        }
    }

    pub fn resolve_call(&mut self, name: &Name) -> Option<(Name, MemberEntry)> {
        let (name, entry) = resolve_alias!(self, .module_members, name)?;
        match &entry {
            MemberEntry::Member(_, _) => Some((name, entry)),
            // For legacy reasons, don't resolve type parameters
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
    pub fn push_alias_scope(&mut self, new_aliases: AliasMapBuilder) {
        let AliasMapBuilder::Namespaced {
            leading_access: new_leading_access,
            module_members: new_module_members,
        } = new_aliases
        else {
            panic!("ICE alias map builder should be namespaced for 2024 paths")
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

impl From<(Name, NumericalAddress)> for AliasEntry {
    fn from((name, addr): (Name, NumericalAddress)) -> Self {
        AliasEntry::Address(name, addr)
    }
}

impl From<(Name, LeadingAccessEntry)> for AliasEntry {
    fn from((name, entry): (Name, LeadingAccessEntry)) -> Self {
        match entry {
            LeadingAccessEntry::Address(addr) => AliasEntry::Address(name, addr),
            LeadingAccessEntry::Module(mident) => AliasEntry::Module(name, mident),
            LeadingAccessEntry::Member(mident, member) => AliasEntry::Member(name, mident, member),
            LeadingAccessEntry::TypeParam => AliasEntry::TypeParam(name),
        }
    }
}

impl From<(Name, MemberEntry)> for AliasEntry {
    fn from((name, entry): (Name, MemberEntry)) -> Self {
        match entry {
            MemberEntry::Member(mident, member) => AliasEntry::Member(name, mident, member),
            MemberEntry::TypeParam => AliasEntry::TypeParam(name),
        }
    }
}

impl UseFunsBuilder {
    pub fn new() -> Self {
        Self {
            explicit: vec![],
            implicit: UniqueMap::new(),
        }
    }
}

//**************************************************************************************************
// Display
//**************************************************************************************************

impl fmt::Debug for AliasEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            AliasEntry::Module(alias, mident) => write!(f, "({alias}, m#{mident})"),
            AliasEntry::Address(alias, addr) => write!(f, "({alias}, @{addr})"),
            AliasEntry::Member(alias, mident, name) => write!(f, "({alias},{mident}::{name})"),
            AliasEntry::TypeParam(alias) => write!(f, "({alias},[tparam])"),
        }
    }
}

impl fmt::Debug for LeadingAccessEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            LeadingAccessEntry::Module(mident) => write!(f, "m#{mident}"),
            LeadingAccessEntry::Address(addr) => write!(f, "@{addr}"),
            LeadingAccessEntry::Member(mident, name) => write!(f, "{mident}::{name}"),
            LeadingAccessEntry::TypeParam => write!(f, "[tparam]"),
        }
    }
}

impl fmt::Debug for MemberEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            MemberEntry::Member(mident, name) => write!(f, "{mident}::{name}"),
            MemberEntry::TypeParam => write!(f, "[tparam]"),
        }
    }
}

impl fmt::Debug for AliasMapBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            AliasMapBuilder::Legacy { modules, members } => {
                writeln!(f, "AliasMapBuilder::Legacy(\n  modules: [")?;
                for (_, key, (target, is_implicit)) in modules {
                    writeln!(f, "    {key} => {target:?} <{is_implicit}>,")?;
                }
                writeln!(f, "  ],\n  members: [")?;
                for (_, key, (target, is_implicit)) in members {
                    writeln!(f, "    {key} => {target:?} <{is_implicit}>,")?;
                }
                writeln!(f, "])")
            }
            AliasMapBuilder::Namespaced {
                leading_access,
                module_members,
            } => {
                writeln!(f, "AliasMapBuilder::Legacy(\n  leading_access: [")?;
                for (_, key, (target, is_implicit)) in leading_access {
                    writeln!(f, "    {key} => {target:?} <{is_implicit}>,")?;
                }
                writeln!(f, "  ],\n  module_members: [")?;
                for (_, key, (target, is_implicit)) in module_members {
                    writeln!(f, "    {key} => {target:?} <{is_implicit}>,")?;
                }
                writeln!(f, "])")
            }
        }
    }
}

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
