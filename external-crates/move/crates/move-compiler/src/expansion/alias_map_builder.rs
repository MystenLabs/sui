// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    expansion::{
        ast::{self as E, ModuleIdent},
        name_validation::ModuleMemberKind,
    },
    parser::ast::{self as P, DocComment},
    shared::{unique_map::UniqueMap, *},
};
use move_ir_types::location::*;
use std::{collections::BTreeSet, fmt};

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

pub type MemberName = (ModuleIdent, Name, ModuleMemberKind);

/// Represents an unnecessary and duplicate alias, where the alias was already in scope
pub struct UnnecessaryAlias {
    pub entry: AliasEntry,
    pub prev: Loc,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AliasEntry {
    Address(Name, NumericalAddress),
    Module(Name, ModuleIdent),
    Member(Name, ModuleIdent, Name),
    TypeParam(Name),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LeadingAccessEntry {
    Address(NumericalAddress),
    Module(ModuleIdent),
    Member(ModuleIdent, Name),
    TypeParam,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
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
    pub doc: DocComment,
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

impl AliasEntry {
    pub fn loc(&self) -> Loc {
        match self {
            AliasEntry::Address(n, _)
            | AliasEntry::Module(n, _)
            | AliasEntry::Member(n, _, _)
            | AliasEntry::TypeParam(n) => n.loc,
        }
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
        match self {
            AliasMapBuilder::Legacy { members, .. } => remove_dup(members, alias),
            AliasMapBuilder::Namespaced {
                leading_access,
                module_members,
            } => match kind {
                // constants and functions are not in the leading access namespace
                ModuleMemberKind::Constant | ModuleMemberKind::Function => {
                    remove_dup(module_members, alias)
                }
                // structs and enums are in the leading access namespace in addition to the module
                // members namespace
                ModuleMemberKind::Struct | ModuleMemberKind::Enum => {
                    let r1 = remove_dup(module_members, alias);
                    let r2 = remove_dup(leading_access, alias);
                    r1.and(r2)
                }
            },
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
        match self {
            AliasMapBuilder::Legacy { members, .. } => members
                .add(alias, ((ident, member, kind), is_implicit))
                .unwrap(),

            AliasMapBuilder::Namespaced {
                leading_access,
                module_members,
            } => match kind {
                // constants and functions are not in the leading access namespace
                ModuleMemberKind::Constant | ModuleMemberKind::Function => {
                    let entry = (MemberEntry::Member(ident, member), is_implicit);
                    module_members.add(alias, entry).unwrap();
                }
                // structs and enums are in the leading access namespace in addition to the module
                // members namespace
                ModuleMemberKind::Struct | ModuleMemberKind::Enum => {
                    let member_entry = (MemberEntry::Member(ident, member), is_implicit);
                    module_members.add(alias, member_entry).unwrap();
                    let leading_access_entry =
                        (LeadingAccessEntry::Member(ident, member), is_implicit);
                    leading_access.add(alias, leading_access_entry).unwrap();
                }
            },
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

    // TODO: the functions below should take a flag indicating if they are from a `use` or local
    // definition for better error reporting.

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
// Debug
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
