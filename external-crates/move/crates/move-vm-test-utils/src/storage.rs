// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Result};
use move_core_types::{
    account_address::AccountAddress,
    effects::{AccountChangeSet, ChangeSet, Op},
    identifier::Identifier,
    language_storage::{ModuleId, StructTag},
    resolver::{LinkageResolver, ModuleResolver, MoveResolver, ResourceResolver},
};
use std::{
    collections::{btree_map, BTreeMap},
    fmt::Debug,
};

/// A dummy storage containing no modules or resources.
#[derive(Debug, Clone)]
pub struct BlankStorage;

impl BlankStorage {
    pub fn new() -> Self {
        Self
    }
}

impl LinkageResolver for BlankStorage {
    type Error = ();
}

impl ModuleResolver for BlankStorage {
    type Error = ();

    fn get_module(&self, _module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(None)
    }
}

impl ResourceResolver for BlankStorage {
    type Error = ();

    fn get_resource(
        &self,
        _address: &AccountAddress,
        _tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(None)
    }
}

/// A storage adapter created by stacking a change set on top of an existing storage backend.
/// This can be used for additional computations without modifying the base.
#[derive(Debug, Clone)]
pub struct DeltaStorage<'a, 'b, S> {
    base: &'a S,
    delta: &'b ChangeSet,
}

impl<'a, 'b, S: LinkageResolver> LinkageResolver for DeltaStorage<'a, 'b, S> {
    type Error = S::Error;

    fn link_context(&self) -> AccountAddress {
        self.base.link_context()
    }

    fn relocate(&self, module_id: &ModuleId) -> std::result::Result<ModuleId, Self::Error> {
        self.base.relocate(module_id)
    }
}

impl<'a, 'b, S: ModuleResolver> ModuleResolver for DeltaStorage<'a, 'b, S> {
    type Error = S::Error;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        if let Some(account_storage) = self.delta.accounts().get(module_id.address()) {
            if let Some(blob_opt) = account_storage.modules().get(module_id.name()) {
                return Ok(blob_opt.clone().ok());
            }
        }

        self.base.get_module(module_id)
    }
}

impl<'a, 'b, S: ResourceResolver> ResourceResolver for DeltaStorage<'a, 'b, S> {
    type Error = S::Error;

    fn get_resource(
        &self,
        _address: &AccountAddress,
        _tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, S::Error> {
        unreachable!()
    }
}

impl<'a, 'b, S: MoveResolver> DeltaStorage<'a, 'b, S> {
    pub fn new(base: &'a S, delta: &'b ChangeSet) -> Self {
        Self { base, delta }
    }
}

/// Simple in-memory storage for modules and resources under an account.
#[derive(Debug, Clone)]
struct InMemoryAccountStorage {
    modules: BTreeMap<Identifier, Vec<u8>>,
}

/// Simple in-memory storage that can be used as a Move VM storage backend for testing purposes.
#[derive(Debug, Clone)]
pub struct InMemoryStorage {
    accounts: BTreeMap<AccountAddress, InMemoryAccountStorage>,
}

fn apply_changes<K, V>(
    map: &mut BTreeMap<K, V>,
    changes: impl IntoIterator<Item = (K, Op<V>)>,
) -> Result<()>
where
    K: Ord + Debug,
{
    use btree_map::Entry::*;
    use Op::*;

    for (k, op) in changes.into_iter() {
        match (map.entry(k), op) {
            (Occupied(entry), New(_)) => {
                bail!(
                    "Failed to apply changes -- key {:?} already exists",
                    entry.key()
                )
            }
            (Occupied(entry), Delete) => {
                entry.remove();
            }
            (Occupied(entry), Modify(val)) => {
                *entry.into_mut() = val;
            }
            (Vacant(entry), New(val)) => {
                entry.insert(val);
            }
            (Vacant(entry), Delete | Modify(_)) => bail!(
                "Failed to apply changes -- key {:?} does not exist",
                entry.key()
            ),
        }
    }
    Ok(())
}

fn get_or_insert<K, V, F>(map: &mut BTreeMap<K, V>, key: K, make_val: F) -> &mut V
where
    K: Ord,
    F: FnOnce() -> V,
{
    use btree_map::Entry::*;

    match map.entry(key) {
        Occupied(entry) => entry.into_mut(),
        Vacant(entry) => entry.insert(make_val()),
    }
}

impl InMemoryAccountStorage {
    fn apply(&mut self, account_changeset: AccountChangeSet) -> Result<()> {
        let modules = account_changeset.into_inner();
        apply_changes(&mut self.modules, modules)?;
        Ok(())
    }

    fn new() -> Self {
        Self {
            modules: BTreeMap::new(),
        }
    }
}

impl InMemoryStorage {
    pub fn apply_extended(&mut self, changeset: ChangeSet) -> Result<()> {
        for (addr, account_changeset) in changeset.into_inner() {
            match self.accounts.entry(addr) {
                btree_map::Entry::Occupied(entry) => {
                    entry.into_mut().apply(account_changeset)?;
                }
                btree_map::Entry::Vacant(entry) => {
                    let mut account_storage = InMemoryAccountStorage::new();
                    account_storage.apply(account_changeset)?;
                    entry.insert(account_storage);
                }
            }
        }

        Ok(())
    }

    pub fn apply(&mut self, changeset: ChangeSet) -> Result<()> {
        self.apply_extended(changeset)
    }

    pub fn new() -> Self {
        Self {
            accounts: BTreeMap::new(),
        }
    }

    pub fn publish_or_overwrite_module(&mut self, module_id: ModuleId, blob: Vec<u8>) {
        let account = get_or_insert(&mut self.accounts, *module_id.address(), || {
            InMemoryAccountStorage::new()
        });
        account.modules.insert(module_id.name().to_owned(), blob);
    }
}

/// Use all default implementations for InMemoryStorage implementation of LinkageResolver
impl LinkageResolver for InMemoryStorage {
    type Error = ();
}

impl ModuleResolver for InMemoryStorage {
    type Error = ();

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        if let Some(account_storage) = self.accounts.get(module_id.address()) {
            return Ok(account_storage.modules.get(module_id.name()).cloned());
        }
        Ok(None)
    }
}

impl ResourceResolver for InMemoryStorage {
    type Error = ();

    fn get_resource(
        &self,
        _address: &AccountAddress,
        _tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        unreachable!()
    }
}
