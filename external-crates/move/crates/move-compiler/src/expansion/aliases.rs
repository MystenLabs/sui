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
pub struct AliasMapBuilder {
    pub modules: UniqueMap<Name, (ModuleIdent, /* is_implicit */ bool)>,
    pub members: UniqueMap<Name, (MemberName, /* is_implicit */ bool)>,
    pub addresses: UniqueMap<Name, (NumericalAddress, /* is_implicit */ bool)>,
    all_aliases_unique: bool,
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

impl AliasMapBuilder {
    /// Create a new AliasMapBuilder. If the all_unique flag is set, names must be unique across
    /// members, modules, and addresses; if it is not, they must only be unique within their
    /// respective kind. This allows us to reuse the builder for both Move 2024 alias resolution
    /// and legacy alias resolution.
    pub fn new(all_aliases_unique: bool) -> Self {
        Self {
            modules: UniqueMap::new(),
            members: UniqueMap::new(),
            addresses: UniqueMap::new(),
            all_aliases_unique,
        }
    }

    pub fn is_empty(&self) -> bool {
        let Self {
            modules,
            members,
            addresses,
            ..
        } = self;
        modules.is_empty() && members.is_empty() && addresses.is_empty()
    }

    fn ensure_all_unique(&mut self, alias: &Name) -> Result<(), Loc> {
        if self.members.get_loc(alias).is_some() {
            self.remove_member_alias_(alias)
        } else if self.modules.get_loc(alias).is_some() {
            self.remove_module_alias_(alias)
        } else if self.addresses.get_loc(alias).is_some() {
            self.remove_address_alias_(alias)
        } else {
            Ok(())
        }
    }

    fn remove_module_alias_(&mut self, alias: &Name) -> Result<(), Loc> {
        let loc = self.modules.get_loc(alias).cloned();
        match self.modules.remove(alias) {
            None => Ok(()),
            Some(_) => Err(loc.unwrap()),
        }
    }

    fn remove_member_alias_(&mut self, alias: &Name) -> Result<(), Loc> {
        let loc = self.members.get_loc(alias).cloned();
        match self.members.remove(alias) {
            None => Ok(()),
            Some(_) => Err(loc.unwrap()),
        }
    }

    fn remove_address_alias_(&mut self, alias: &Name) -> Result<(), Loc> {
        let loc = self.addresses.get_loc(alias).cloned();
        match self.addresses.remove(alias) {
            None => Ok(()),
            Some(_) => Err(loc.unwrap()),
        }
    }

    fn remove_module_alias(&mut self, alias: &Name) -> Result<(), Loc> {
        if self.all_aliases_unique {
            self.ensure_all_unique(alias)
        } else {
            self.remove_module_alias_(alias)
        }
    }

    fn remove_member_alias(&mut self, alias: &Name) -> Result<(), Loc> {
        if self.all_aliases_unique {
            self.ensure_all_unique(alias)
        } else {
            self.remove_member_alias_(alias)
        }
    }

    fn remove_address_alias(&mut self, alias: &Name) -> Result<(), Loc> {
        if self.all_aliases_unique {
            self.ensure_all_unique(alias)
        } else {
            self.remove_address_alias_(alias)
        }
    }

    /// Adds a module alias to the map.
    /// Errors if one already bound for that alias
    pub fn add_module_alias(&mut self, alias: Name, ident: ModuleIdent) -> Result<(), Loc> {
        let result = self.remove_module_alias_(&alias);
        self.modules
            .add(alias, (ident, /* is_implicit */ false))
            .unwrap();
        result
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
        let result = self.remove_member_alias(&alias);
        self.members
            .add(alias, ((ident, member, kind), /* is_implicit */ false))
            .unwrap();
        result
    }

    /// Adds an address alias to the map.
    /// Errors if one already bound for that alias
    pub fn add_address_alias(&mut self, alias: Name, address: NumericalAddress) -> Result<(), Loc> {
        let result = self.remove_address_alias(&alias);
        self.addresses
            .add(alias, (address, /* is_implicit */ false))
            .unwrap();
        result
    }

    /// Same as `add_module_alias` but it does not update the scope, and as such it will not be
    /// reported as unused
    pub fn add_implicit_module_alias(
        &mut self,
        alias: Name,
        ident: ModuleIdent,
    ) -> Result<(), Loc> {
        let result = self.remove_module_alias(&alias);
        self.modules
            .add(alias, (ident, /* is_implicit */ true))
            .unwrap();
        result
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
        let result = self.remove_member_alias_(&alias);
        self.members
            .add(alias, ((ident, member, kind), /* is_implicit */ true))
            .unwrap();
        result
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
        let mut new_map = Self::new();
        let AliasMapBuilder {
            addresses,
            modules,
            members,
            all_aliases_unique,
        } = new_aliases;

        // addresses
        for (alias, (entry, is_implicit)) in addresses {
            let entry = LeadingAccessEntry::Address(entry);
            new_map.leading_access.add(alias, entry).unwrap();
            if !is_implicit {
                let alias_entry = (alias, entry).into();
                new_map.unused.insert(alias_entry);
            }
        }

        // leading access
        for (alias, (entry, is_implicit)) in modules {
            let entry = LeadingAccessEntry::Module(entry);
            let prev = new_map.leading_access.add(alias, entry);
            // all_aliases_unique ==> prev.is_ok()
            assert!(!all_aliases_unique || prev.is_ok());
            if !is_implicit {
                let alias_entry = (alias, entry).into();
                new_map.unused.insert(alias_entry);
            }
        }
        // Should we just exclude these and just include enums?
        let member_leading_access = members
            .key_cloned_iter()
            .filter(|(_, ((_, _, kind), _))| matches!(kind, ModuleMemberKind::Struct));
        for (alias, ((mident, name, _kind), is_implicit)) in member_leading_access {
            let entry = LeadingAccessEntry::Member(*mident, *name);
            let prev = new_map.leading_access.add(alias, entry);
            // all_aliases_unique ==> prev.is_ok()
            assert!(!all_aliases_unique || prev.is_ok());
            if !is_implicit {
                let alias_entry = (alias, entry).into();
                new_map.unused.insert(alias_entry);
            }
        }

        // module members
        for (alias, ((mident, name, _kind), is_implicit)) in members {
            let entry = MemberEntry::Member(mident, name);
            new_map.module_members.add(alias, entry).unwrap();
            if !is_implicit {
                let alias_entry = (alias, entry).into();
                new_map.unused.insert(alias_entry);
            }
        }

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
            new_map
                .leading_access
                .add(*tparam, LeadingAccessEntry::TypeParam)
                .unwrap();
            new_map
                .module_members
                .add(*tparam, MemberEntry::TypeParam)
                .unwrap();
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
        let Self {
            modules,
            members,
            addresses,
            all_aliases_unique,
        } = self;
        writeln!(
            f,
            "AliasMapBuilder(\n  all_aliases_unique: {all_aliases_unique},\n  members: ["
        )?;
        for (_, key, (target, is_implicit)) in members {
            writeln!(f, "    {key} => {target:?} <{is_implicit}>,")?;
        }
        writeln!(f, "],\n  modules: [")?;
        for (_, key, (target, is_implicit)) in modules {
            writeln!(f, "    {key} => {target:?} <{is_implicit}>,")?;
        }
        writeln!(f, ",\n addresses: [")?;
        for (_, key, (target, is_implicit)) in addresses {
            writeln!(f, "    {key} => {target:?} <{is_implicit}>,")?;
        }
        writeln!(f, "])")
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
