// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    expansion::ast::{self as E, ModuleIdent},
    naming::{ast as N, name_resolver::ResolvedDefinition},
    parser::ast as P,
    shared::{unique_map::UniqueMap, *},
};
use move_ir_types::location::*;
use std::fmt;

use super::aliases::NameMapKind;

#[derive(Clone)]
pub enum AliasMapBuilder {
    Legacy {
        modules: UniqueMap<Name, (ModuleIdent, /* is_implicit */ bool)>,
        members: UniqueMap<Name, (ResolvedDefinition, /* is_implicit */ bool)>,
    },
    Namespaced {
        leading_access: UniqueMap<Name, (LeadingAccessEntry, /* is_implicit */ bool)>,
        module_members: UniqueMap<Name, (ResolvedDefinition, /* is_implicit */ bool)>,
        kind: NameMapKind,
    },
}

/// Represents an unnecessary and duplicate alias, where the alias was already in scope
pub struct UnnecessaryAlias {
    pub entry: AliasEntry,
    pub prev: Loc,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AliasEntry {
    Address(Name, NumericalAddress),
    Module(Name, ModuleIdent),
    Definition(Name, ResolvedDefinition),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LeadingAccessEntry {
    Address(NumericalAddress),
    Module(ModuleIdent),
    // Datatypes and Type Parameters
    Member(ResolvedDefinition),
}

#[derive(Clone, Copy)]
pub enum NameSpace {
    LeadingAccess,
    ModuleMembers,
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
    pub implicit: UniqueMap<Name, N::ImplicitUseFunCandidate>,
}

impl AliasEntry {
    pub fn loc(&self) -> Loc {
        match self {
            AliasEntry::Address(n, _)
            | AliasEntry::Module(n, _)
            | AliasEntry::Definition(n, _)
            | AliasEntry::TypeParam(n, _) => n.loc,
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

    pub fn namespaced(kind: NameMapKind) -> Self {
        Self::Namespaced {
            leading_access: UniqueMap::new(),
            module_members: UniqueMap::new(),
            kind,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Legacy { modules, members } => modules.is_empty() && members.is_empty(),
            Self::Namespaced {
                leading_access,
                module_members,
                kind: _,
            } => leading_access.is_empty() && module_members.is_empty(),
        }
    }

    fn remove_module_alias(&mut self, alias: &Name) -> Result<(), Loc> {
        match self {
            Self::Legacy { modules, .. } => remove_dup(modules, alias),
            Self::Namespaced { leading_access, .. } => remove_dup(leading_access, alias),
        }
    }

    fn remove_member_alias(&mut self, alias: &Name, defn: &ResolvedDefinition) -> Result<(), Loc> {
        match self {
            AliasMapBuilder::Legacy { members, .. } => remove_dup(members, alias),
            AliasMapBuilder::Namespaced {
                leading_access,
                module_members,
                kind: _,
            } => match defn {
                // constants and functions are not in the leading access namespace
                ResolvedDefinition::Function(_)
                | ResolvedDefinition::Constant(_)
                | ResolvedDefinition::BuiltinFun(_) => remove_dup(module_members, alias),
                // structs, enums, and type parameters are in the leading access namespace in
                // addition to the module members namespace so that they shadow path prefixes
                ResolvedDefinition::Datatype(_)
                | ResolvedDefinition::TypeParam(_, _)
                | ResolvedDefinition::BuiltinType(_) => {
                    let r1 = remove_dup(module_members, alias);
                    let r2 = remove_dup(leading_access, alias);
                    r1.and(r2)
                }
                // we do not support variant aliases
                ResolvedDefinition::Variant(_) => unreachable!(),
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
        defn: ResolvedDefinition,
        is_implicit: bool,
    ) -> Result<(), Loc> {
        let result = self.remove_member_alias(&alias, &defn);
        match self {
            AliasMapBuilder::Legacy { members, .. } => {
                members.add(alias, (defn, is_implicit)).unwrap()
            }
            AliasMapBuilder::Namespaced {
                leading_access,
                module_members,
                kind: _,
            } => match &defn {
                // constants and functions are not in the leading access namespace
                ResolvedDefinition::Function(_)
                | ResolvedDefinition::Constant(_)
                | ResolvedDefinition::BuiltinFun(_) => {
                    let entry = (defn, is_implicit);
                    module_members.add(alias, entry).unwrap();
                }
                // structs, enums, and type parameters are in the leading access namespace in
                // addition to the module members namespace so that they shadow path prefixes
                ResolvedDefinition::Datatype(_)
                | ResolvedDefinition::TypeParam(_, _)
                | ResolvedDefinition::BuiltinType(_) => {
                    let member_entry = (defn, is_implicit);
                    module_members.add(alias, member_entry).unwrap();
                    let leading_access_entry = (LeadingAccessEntry::Member(defn), is_implicit);
                    leading_access.add(alias, leading_access_entry).unwrap();
                }
                // we do not support variant aliases
                ResolvedDefinition::Variant(_) => unreachable!(),
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

    /// Adds a module alias to the map.
    /// Errors if one already bound for that alias
    pub fn add_module_alias(&mut self, alias: Name, ident: ModuleIdent) -> Result<(), Loc> {
        self.add_module_alias_(alias, ident, /* is_implicit */ false)
    }

    /// Adds a member alias to the map.
    /// Errors if one already bound for that alias
    pub fn add_member_alias(&mut self, alias: Name, defn: ResolvedDefinition) -> Result<(), Loc> {
        self.add_member_alias_(alias, defn, /* is_implicit */ false)
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
        defn: ResolvedDefinition,
    ) -> Result<(), Loc> {
        self.add_member_alias_(alias, defn, /* is_implicit */ true)
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
            LeadingAccessEntry::Member(member) => AliasEntry::Definition(name, member),
        }
    }
}

impl From<(Name, ResolvedDefinition)> for AliasEntry {
    fn from((name, entry): (Name, ResolvedDefinition)) -> Self {
        AliasEntry::Definition(name, entry)
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
            AliasEntry::Definition(alias, entry) => {
                write!(f, "({alias},{}::{})", entry.mident(), entry.name())
            }
            AliasEntry::TypeParam(alias, tparam) => write!(f, "({alias},[{tparam}])"),
        }
    }
}

impl fmt::Debug for LeadingAccessEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            LeadingAccessEntry::Module(mident) => write!(f, "m#{mident}"),
            LeadingAccessEntry::Address(addr) => write!(f, "@{addr}"),
            LeadingAccessEntry::Member(entry) => write!(f, "{}::{}", entry.mident(), entry.name()),
            LeadingAccessEntry::TypeParam(tparam) => write!(f, "[{tparam}]"),
        }
    }
}

impl fmt::Debug for AliasMapBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            AliasMapBuilder::Legacy { modules, members } => {
                writeln!(f, "AliasMapBuilder::Legacy(\n  modules: [")?;
                for (_, key, (target, is_implicit)) in modules {
                    writeln!(f, "    {key} => {target} <{is_implicit}>,")?;
                }
                writeln!(f, "  ],\n  members: [")?;
                for (_, key, (target, is_implicit)) in members {
                    writeln!(f, "    {key} => {target} <{is_implicit}>,")?;
                }
                writeln!(f, "])")
            }
            AliasMapBuilder::Namespaced {
                leading_access,
                module_members,
                kind,
            } => {
                writeln!(
                    f,
                    "AliasMapBuilder::Namespaced( {kind:?}\n  leading_access: ["
                )?;
                for (_, key, (target, is_implicit)) in leading_access {
                    writeln!(f, "    {key} => {target} <{is_implicit}>,")?;
                }
                writeln!(f, "  ],\n  module_members: [")?;
                for (_, key, (target, is_implicit)) in module_members {
                    writeln!(f, "    {key} => {target} <{is_implicit}>,")?;
                }
                writeln!(f, "])")
            }
        }
    }
}
