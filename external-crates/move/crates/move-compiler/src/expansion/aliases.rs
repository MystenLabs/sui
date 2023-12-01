// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    expansion::ast::{self as E, ModuleIdent},
    parser::ast::{self as P},
    shared::{unique_map::UniqueMap, unique_set::UniqueSet, *},
};
use move_ir_types::location::*;
use std::fmt;

#[derive(Clone, Debug)]
pub struct AliasSet {
    pub modules: UniqueSet<Name>,
    pub members: UniqueSet<Name>,
}

#[derive(Clone)]
pub struct AliasMapBuilder {
    pub modules: UniqueMap<Name, (ModuleIdent, /* is_implicit */ bool)>,
    pub members: UniqueMap<Name, ((ModuleIdent, Name), /* is_implicit */ bool)>,
    pub addresses: UniqueMap<Name, (NumericalAddress, /* is_implicit */ bool)>,
    all_aliases_unique: bool,
}

#[derive(Clone)]
pub enum AliasEntry {
    Address(NumericalAddress),
    Module(ModuleIdent),
    Member(ModuleIdent, Name),
    TypeParam,
}

pub struct AliasMap {
    aliases: UniqueMap<Name, (AliasEntry, /* used */ bool)>,
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
    ) -> Result<(), Loc> {
        let result = self.remove_member_alias(&alias);
        self.members
            .add(alias, ((ident, member), /* is_implicit */ false))
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
    ) -> Result<(), Loc> {
        let result = self.remove_member_alias_(&alias);
        self.members
            .add(alias, ((ident, member), /* is_implicit */ true))
            .unwrap();
        result
    }
}

impl AliasMap {
    pub fn new() -> Self {
        Self {
            aliases: UniqueMap::new(),
            previous: None,
        }
    }

    pub fn get(&mut self, name: &Name) -> Option<AliasEntry> {
        if let Some((entry, used)) = self.aliases.get_mut(name) {
            *used = true;
            // Type parameters are never resolved. We track them to apply appropriate shadowing.
            if matches!(entry, AliasEntry::TypeParam) {
                return None;
            } else {
                return Some(entry.clone());
            }
        }

        let mut current_scope: Option<&mut Box<AliasMap>> = self.previous.as_mut();
        while let Some(scope) = current_scope {
            if let Some((entry, used)) = scope.aliases.get_mut(name) {
                *used = true;
                // Type parameters are never resolved. We track them to apply appropriate shadowing.
                if matches!(entry, AliasEntry::TypeParam) {
                    return None;
                } else {
                    return Some(entry.clone());
                }
            }
            current_scope = scope.previous.as_mut();
        }

        None
    }

    /// Pushes a new scope, adding all of the new items to it (shadowing the outer one).
    /// Returns any name collisions that occur between addresses, members, and modules in the map
    /// builder.
    pub fn push_alias_scope(&mut self, new_aliases: AliasMapBuilder) {
        let mut new_map = UniqueMap::new();
        for (alias, ((mident, name), is_implicit)) in new_aliases.members {
            new_map
                .add(alias, (AliasEntry::Member(mident, name), is_implicit))
                .unwrap();
        }
        for (alias, (entry, is_implicit)) in new_aliases.modules {
            new_map
                .add(alias, (AliasEntry::Module(entry), is_implicit))
                .unwrap();
        }
        for (alias, (entry, is_implicit)) in new_aliases.addresses {
            new_map
                .add(alias, (AliasEntry::Address(entry), is_implicit))
                .unwrap();
        }
        let previous = std::mem::replace(
            self,
            Self {
                aliases: new_map,
                previous: None,
            },
        );
        self.previous = Some(Box::new(previous));
    }

    /// Similar to add_and_shadow but just hides aliases now shadowed by a type parameter.
    /// Type parameters are never resolved. We track them to apply appropriate shadowing.
    pub fn push_type_parameters<'a, I: IntoIterator<Item = &'a Name>>(&mut self, tparams: I)
    where
        I::IntoIter: ExactSizeIterator,
    {
        let mut aliases = UniqueMap::new();
        for tparam in tparams {
            let _ = aliases.add(*tparam, (AliasEntry::TypeParam, true));
        }
        let previous = std::mem::replace(
            self,
            Self {
                aliases,
                previous: None,
            },
        );
        self.previous = Some(Box::new(previous));
    }

    /// Resets the alias map to the previous scope, and returns the set of unused aliases
    pub fn pop_scope(&mut self) -> AliasSet {
        let mut result = AliasSet::new();
        for (name, (entry, used)) in self.aliases.key_cloned_iter() {
            if !used {
                match entry {
                    AliasEntry::Module(_) => result.modules.add(name).unwrap(),
                    AliasEntry::Member(_, _) => result.members.add(name).unwrap(),
                    AliasEntry::Address(_) | AliasEntry::TypeParam => (),
                }
            }
        }
        if self.previous.is_some() {
            let previous = std::mem::take(&mut self.previous).unwrap();
            let _ = std::mem::replace(self, *previous);
        } else {
            self.aliases = UniqueMap::new();
        }
        result
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
            AliasEntry::Module(name) => write!(f, "m#{}", name),
            AliasEntry::Address(addr) => write!(f, "@{}", addr),
            AliasEntry::Member(mident, name) => write!(f, "{}::{}", mident, name),
            AliasEntry::TypeParam => write!(f, "[tparam]"),
        }
    }
}

impl fmt::Debug for AliasMapBuilder {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "Map(members: [")?;
        for (_, key, (target, is_implicit)) in &self.members {
            write!(f, "{} => {:?} <{}>, ", key, target, is_implicit)?;
        }
        write!(f, ", modules: [")?;
        for (_, key, (target, is_implicit)) in &self.modules {
            write!(f, "{} => {:?} <{}>, ", key, target, is_implicit)?;
        }
        write!(f, ", addresses: [")?;
        for (_, key, (target, is_implicit)) in &self.addresses {
            write!(f, "{} => {:?} <{}>, ", key, target, is_implicit)?;
        }
        write!(f, "])")
    }
}

impl fmt::Debug for AliasMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "Map(aliases: [")?;
        for (_, key, (_, value)) in &self.aliases {
            write!(f, "{} => {:?}, ", key, value)?;
        }
        write!(f, "], prev: {:?} )", self.previous)
    }
}
