// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    shared::{linkage_context::LinkageContext, types::PackageStorageId},
    validation::verification::ast as verif_ast,
};
use anyhow::Result;
use move_binary_format::CompiledModule;
use move_core_types::{
    account_address::AccountAddress,
    effects::ChangeSet,
    identifier::Identifier,
    language_storage::ModuleId,
    resolver::{ModuleResolver, MoveResolver, SerializedPackage, TypeOrigin},
};
use std::collections::BTreeMap;

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

/// Simple in-memory representation of packages
#[derive(Debug, Clone)]
pub struct StoredPackage {
    pub modules: BTreeMap<Identifier, Vec<u8>>,
    /// For each dependency (including transitive dependencies), maps runtime package ID to the
    /// storage ID of the package that is to be used for the linkage rooted at this package.
    pub linkage_context: LinkageContext,
    /// The type origin table for the package. Every type in the package must have an entry in this
    /// table.
    pub type_origin_table: Vec<TypeOrigin>,
}

/// Simple in-memory storage that can be used as a Move VM storage backend for testing purposes.
#[derive(Debug, Clone)]
pub struct InMemoryStorage {
    accounts: BTreeMap<PackageStorageId, StoredPackage>,
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

impl StoredPackage {
    fn empty(storage_id: AccountAddress) -> Self {
        Self {
            modules: BTreeMap::new(),
            linkage_context: LinkageContext::new(storage_id, BTreeMap::new()),
            type_origin_table: vec![],
        }
    }

    pub fn from_modules_for_testing(
        storage_id: AccountAddress,
        modules: Vec<CompiledModule>,
    ) -> Result<Self> {
        assert!(!modules.is_empty());
        // Map the modules in this package to `storage_id` and generate the identity linkage for
        // all deps.
        let mut linkage_table = BTreeMap::new();
        let type_origin_table = generate_type_origins(storage_id, &modules);
        let modules: BTreeMap<_, _> = modules
            .into_iter()
            .map(|m| {
                let mut bin = vec![];
                linkage_table.insert(*m.self_id().address(), storage_id);
                for addr in m
                    .immediate_dependencies()
                    .iter()
                    .map(|dep| *dep.address())
                    .filter(|addr| *addr != *m.self_id().address())
                {
                    linkage_table.insert(addr, addr);
                }
                m.serialize_with_version(m.version, &mut bin)?;
                Ok((m.self_id().name().to_owned(), bin))
            })
            .collect::<Result<_>>()?;

        Ok(Self {
            modules,
            linkage_context: LinkageContext::new(storage_id, linkage_table),
            type_origin_table,
        })
    }

    pub fn from_module_for_testing_with_linkage(
        storage_id: AccountAddress,
        linkage_context: LinkageContext,
        modules: Vec<CompiledModule>,
    ) -> Result<Self> {
        let type_origin_table = generate_type_origins(storage_id, &modules);
        let modules: BTreeMap<_, _> = modules
            .into_iter()
            .map(|m| {
                let mut bin = vec![];
                m.serialize_with_version(m.version, &mut bin)?;
                Ok((m.self_id().name().to_owned(), bin))
            })
            .collect::<Result<_>>()?;

        Ok(Self {
            modules,
            linkage_context,
            type_origin_table,
        })
    }

    pub fn from_verified_package(verified_package: verif_ast::Package) -> Self {
        Self {
            modules: verified_package
                .as_modules()
                .into_iter()
                .map(|m| {
                    let dm = m.to_compiled_module();
                    let name = dm.self_id().name().to_owned();
                    let mut serialized = vec![];
                    dm.serialize_with_version(dm.version, &mut serialized)
                        .unwrap();
                    (name, serialized)
                })
                .collect(),
            linkage_context: LinkageContext::new(
                verified_package.storage_id,
                verified_package.linkage_table.into_iter().collect(),
            ),
            type_origin_table: verified_package.type_origin_table,
        }
    }

    pub fn into_serialized_package(self) -> SerializedPackage {
        SerializedPackage {
            storage_id: self.linkage_context.root_package(),
            modules: self.modules.into_values().collect(),
            linkage_table: self.linkage_context.linkage_table.into_iter().collect(),
            type_origin_table: self.type_origin_table,
        }
    }
}

pub fn generate_type_origins(
    storage_id: PackageStorageId,
    modules: &[CompiledModule],
) -> Vec<TypeOrigin> {
    modules
        .iter()
        .flat_map(|module| {
            module
                .struct_defs()
                .iter()
                .map(|def| {
                    let mid = module.self_id();
                    let handle = module.datatype_handle_at(def.struct_handle);
                    let struct_name = module.identifier_at(handle.name).to_owned();
                    TypeOrigin {
                        module_name: mid.name().to_owned(),
                        type_name: struct_name.clone(),
                        origin_id: storage_id,
                    }
                })
                .chain(module.enum_defs().iter().map(|def| {
                    let mid = module.self_id();
                    let handle = module.datatype_handle_at(def.enum_handle);
                    let enum_name = module.identifier_at(handle.name);
                    TypeOrigin {
                        module_name: mid.name().to_owned(),
                        type_name: enum_name.to_owned(),
                        origin_id: storage_id,
                    }
                }))
        })
        .collect()
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            accounts: BTreeMap::new(),
        }
    }

    pub fn publish_package(&mut self, stored_package: StoredPackage) {
        self.accounts.insert(
            stored_package.linkage_context.root_package(),
            stored_package,
        );
    }

    pub fn publish_or_overwrite_module(
        &mut self,
        storage_id: PackageStorageId,
        module_name: Identifier,
        blob: Vec<u8>,
    ) {
        let account = self
            .accounts
            .entry(storage_id)
            .or_insert_with(|| StoredPackage::empty(storage_id));
        account.modules.insert(module_name, blob);
    }

    pub fn debug_dump(&self) {
        for (storage_id, stored_package) in &self.accounts {
            println!("Storage ID: {:?}", storage_id);
            println!("Linkage context: {:?}", stored_package.linkage_context);
            println!("Type origins: {:?}", stored_package.type_origin_table);
            println!("Modules:");
            for module_name in stored_package.modules.keys() {
                println!("\tModule: {:?}", module_name);
            }
        }
    }
}

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
                if let Some(stored_package) = self.accounts.get(storage_id) {
                    Ok(Some(stored_package.clone().into_serialized_package()))
                } else {
                    Ok(None)
                }
            })
            .collect::<Result<Vec<_>, Self::Error>>()
    }
}
