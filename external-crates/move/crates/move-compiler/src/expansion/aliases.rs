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

#[derive(Clone)]
pub enum AliasEntry {
    Address(NumericalAddress),
    Module(ModuleIdent),
    Member(ModuleIdent, Name),
    TypeParam,
}

#[derive(Clone)]
pub struct AliasMapBuilder {
    aliases: UniqueMap<Name, (AliasEntry, /* is_implicit */ bool)>,
}

#[derive(Clone, Debug)]
pub struct AliasSet {
    pub modules: UniqueSet<Name>,
    pub members: UniqueSet<Name>,
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
    pub fn new() -> Self {
        Self {
            aliases: UniqueMap::new(),
        }
    }

    fn ensure_no_alias(&mut self, alias: &Name) -> Result<(), Loc> {
        let loc = self.aliases.get_loc(alias).cloned();
        match self.aliases.remove(alias) {
            None => Ok(()),
            Some(_) => Err(loc.unwrap()),
        }
    }

    fn add_member(&mut self, alias: Name, entry: AliasEntry, is_implicit: bool) -> Result<(), Loc> {
        let result = self.ensure_no_alias(&alias);
        self.aliases
            .add(alias, (entry, /* is_implicit */ is_implicit))
            .unwrap();
        result
    }

    /// Adds a address alias to the map.
    /// Errors if one already bound for that alias
    pub fn add_address_alias(&mut self, alias: Name, address: NumericalAddress) -> Result<(), Loc> {
        self.add_member(alias, AliasEntry::Address(address), false)
    }

    /// Adds a module alias to the map.
    /// Errors if one already bound for that alias
    pub fn add_module_alias(&mut self, alias: Name, ident: ModuleIdent) -> Result<(), Loc> {
        self.add_member(alias, AliasEntry::Module(ident), false)
    }

    /// Adds a member alias to the map.
    /// Errors if one already bound for that alias
    pub fn add_member_alias(
        &mut self,
        alias: Name,
        ident: ModuleIdent,
        member: Name,
    ) -> Result<(), Loc> {
        self.add_member(alias, AliasEntry::Member(ident, member), false)
    }

    /// Adds a address alias to the map.
    /// Errors if one already bound for that alias
    #[allow(unused)]
    pub fn add_implicit_address_alias(
        &mut self,
        alias: Name,
        address: NumericalAddress,
    ) -> Result<(), Loc> {
        self.add_member(alias, AliasEntry::Address(address), true)
    }

    /// Same as `add_module_alias` but it does not update the scope, and as such it will not be
    /// reported as unused
    pub fn add_implicit_module_alias(
        &mut self,
        alias: Name,
        ident: ModuleIdent,
    ) -> Result<(), Loc> {
        self.add_member(alias, AliasEntry::Module(ident), true)
    }

    /// Same as `add_member_alias` but it does not update the scope, and as such it will not be
    /// reported as unused
    pub fn add_implicit_member_alias(
        &mut self,
        alias: Name,
        ident: ModuleIdent,
        member: Name,
    ) -> Result<(), Loc> {
        self.add_member(alias, AliasEntry::Member(ident, member), true)
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
                None
            } else {
                Some(entry.clone())
            }
        } else if let Some(previous) = self.previous.as_mut() {
            previous.get(name)
        } else {
            None
        }
    }

    /// Pushes a new scope, adding all of the new items to it (shadowing the outer one).
    pub fn push_alias_scope(&mut self, new_aliases: AliasMapBuilder) {
        let mut new_map = UniqueMap::new();
        for (alias, (entry, is_implicit)) in new_aliases.aliases {
            new_map.add(alias, (entry, is_implicit)).unwrap();
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
        self.push_alias_scope(AliasMapBuilder { aliases })
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
        write!(f, "Map(aliases: [")?;
        for (_, key, (target, is_implicit)) in &self.aliases {
            write!(f, "{} => {:?} <{}>, ", key, target, is_implicit)?;
        }
        write!(f, "], unused: [")?;
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
