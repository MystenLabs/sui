// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Result};
use move_core_types::{
    account_address::AccountAddress,
    effects::{AccountChangeSet, ChangeSet, Op},
    identifier::Identifier,
    language_storage::ModuleId,
    resolver::{ModuleResolver, MoveResolver, SerializedPackage},
};
use std::{
    collections::{btree_map, BTreeMap},
    fmt::Debug,
};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// A dummy storage containing no modules or resources.
#[derive(Debug, Clone)]
pub struct BlankStorage;

/// A storage adapter created by stacking a change set on top of an existing storage backend.
/// This can be used for additional computations without modifying the base.
#[derive(Debug, Clone)]
pub struct DeltaStorage<'a, 'b, S> {
    base: &'a S,
    delta: &'b ChangeSet,
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

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl BlankStorage {
    pub fn new() -> Self {
        Self
    }
}

impl<'a, 'b, S: MoveResolver> DeltaStorage<'a, 'b, S> {
    pub fn new(base: &'a S, delta: &'b ChangeSet) -> Self {
        Self { base, delta }
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

// -------------------------------------------------------------------------------------------------
// Resolver Impls
// -------------------------------------------------------------------------------------------------

// -----------------------------------------------
// Module Resolvers
// -----------------------------------------------

impl ModuleResolver for BlankStorage {
    type Error = ();

    fn get_module(&self, _module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(None)
    }

    fn get_packages_static<const N: usize>(
        &self,
        ids: [AccountAddress; N],
    ) -> std::prelude::v1::Result<[Option<SerializedPackage>; N], Self::Error> {
        self.get_packages(&ids).map(|packages| {
            packages
                .try_into()
                .expect("Impossible to get a length mismatch")
        })
    }

    fn get_packages(
        &self,
        ids: &[AccountAddress],
    ) -> Result<Vec<Option<SerializedPackage>>, Self::Error> {
        Ok(ids.iter().map(|_| None).collect())
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

    fn get_packages_static<const N: usize>(
        &self,
        ids: [AccountAddress; N],
    ) -> std::prelude::v1::Result<[Option<SerializedPackage>; N], Self::Error> {
        self.get_packages(&ids).map(|packages| {
            packages
                .try_into()
                .expect("Impossible to get a length mismatch")
        })
    }

    fn get_packages(
        &self,
        ids: &[AccountAddress],
    ) -> Result<Vec<Option<SerializedPackage>>, Self::Error> {
        ids.iter()
            .map(|storage_id| {
                if let Some(account_storage) = self.delta.accounts().get(storage_id) {
                    let module_bytes: Vec<_> = account_storage
                        .modules()
                        .values()
                        .map(|op| op.clone().ok())
                        .collect::<Option<_>>()
                        .unwrap_or_default();

                    Ok(Some(SerializedPackage::raw_package(
                        module_bytes,
                        *storage_id,
                    )))
                } else {
                    // TODO: Can optimize this to do a two-pass bulk lookup if we want
                    Ok(self.base.get_packages(&[*storage_id])?[0].clone())
                }
            })
            .collect::<Result<Vec<_>, Self::Error>>()
    }
}

impl ModuleResolver for InMemoryStorage {
    type Error = ();

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        if let Some(account_storage) = self.accounts.get(module_id.address()) {
            return Ok(account_storage.modules.get(module_id.name()).cloned());
        }
        Ok(None)
    }

    fn get_packages_static<const N: usize>(
        &self,
        ids: [AccountAddress; N],
    ) -> std::prelude::v1::Result<[Option<SerializedPackage>; N], Self::Error> {
        self.get_packages(&ids).map(|packages| {
            packages
                .try_into()
                .expect("Impossible to get a length mismatch")
        })
    }

    fn get_packages(
        &self,
        ids: &[AccountAddress],
    ) -> Result<Vec<Option<SerializedPackage>>, Self::Error> {
        ids.iter()
            .map(|storage_id| {
                if let Some(account_storage) = self.accounts.get(storage_id) {
                    let module_bytes: Vec<_> = account_storage
                        .modules
                        .values()
                        .map(|op| op.clone())
                        .collect();

                    Ok(Some(SerializedPackage::raw_package(
                        module_bytes,
                        *storage_id,
                    )))
                } else {
                    Ok(None)
                }
            })
            .collect::<Result<Vec<_>, Self::Error>>()
    }
}

// -------------------------------------------------------------------------------------------------
// Helper Functions
// -------------------------------------------------------------------------------------------------

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
