// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    shared::{
        linkage_context::LinkageContext,
        types::{DefiningTypeId, OriginalId},
    },
    validation::verification::ast as verif_ast,
};
use anyhow::Result;
use move_binary_format::CompiledModule;
use move_core_types::{
    account_address::AccountAddress,
    effects::ChangeSet,
    identifier::Identifier,
    resolver::{ModuleResolver, SerializedPackage, TypeOrigin},
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
    /// The package ID (address) for this package.
    pub version_id: DefiningTypeId,
    pub original_id: OriginalId,
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
    accounts: BTreeMap<DefiningTypeId, StoredPackage>,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl BlankStorage {
    pub fn new() -> Self {
        Self
    }
}

impl<'a, 'b, S: ModuleResolver> DeltaStorage<'a, 'b, S> {
    pub fn new(base: &'a S, delta: &'b ChangeSet) -> Self {
        Self { base, delta }
    }
}

impl StoredPackage {
    fn empty(original_id: OriginalId, version_id: DefiningTypeId) -> Self {
        Self {
            version_id,
            original_id,
            modules: BTreeMap::new(),
            linkage_context: LinkageContext::new(BTreeMap::new()),
            type_origin_table: vec![],
        }
    }

    pub fn from_modules_for_testing(
        version_id: DefiningTypeId,
        modules: Vec<CompiledModule>,
    ) -> Result<Self> {
        assert!(!modules.is_empty());
        // Map the modules in this package to `version_id` and generate the identity linkage for
        // all deps.
        let mut linkage_table = BTreeMap::new();
        let type_origin_table = generate_type_origins(version_id, &modules);
        let modules: BTreeMap<_, _> = modules
            .into_iter()
            .map(|m| {
                let mut bin = vec![];
                linkage_table.insert(*m.self_id().address(), version_id);
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

        let linkage_context = LinkageContext::new(linkage_table);
        Ok(Self {
            version_id,
            original_id: Self::original_id(&linkage_context, version_id),
            modules,
            linkage_context,
            type_origin_table,
        })
    }

    pub fn from_module_for_testing_with_linkage(
        version_id: DefiningTypeId,
        linkage_context: LinkageContext,
        modules: Vec<CompiledModule>,
    ) -> Result<Self> {
        let type_origin_table = generate_type_origins(version_id, &modules);
        let modules: BTreeMap<_, _> = modules
            .into_iter()
            .map(|m| {
                let mut bin = vec![];
                m.serialize_with_version(m.version, &mut bin)?;
                Ok((m.self_id().name().to_owned(), bin))
            })
            .collect::<Result<_>>()?;

        Ok(Self {
            version_id,
            original_id: Self::original_id(&linkage_context, version_id),
            modules,
            linkage_context,
            type_origin_table,
        })
    }

    pub fn from_verified_package(verified_package: verif_ast::Package) -> Self {
        Self {
            version_id: verified_package.version_id,
            original_id: verified_package.original_id,
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
            linkage_context: LinkageContext::new(verified_package.linkage_table),
            type_origin_table: verified_package.type_origin_table,
        }
    }

    pub fn into_serialized_package(self) -> SerializedPackage {
        SerializedPackage {
            storage_id: self.version_id,
            runtime_id: self.original_id,
            modules: self.modules,
            linkage_table: self.linkage_context.linkage_table.into_iter().collect(),
            type_origin_table: self.type_origin_table,
        }
    }

    fn original_id(linkage: &LinkageContext, version_id: DefiningTypeId) -> OriginalId {
        linkage
            .linkage_table
            .iter()
            .find_map(|(k, v)| if *v == version_id { Some(*k) } else { None })
            .expect("address not found in linkage table")
    }
}

pub fn generate_type_origins(
    version_id: DefiningTypeId,
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
                        origin_id: version_id,
                    }
                })
                .chain(module.enum_defs().iter().map(|def| {
                    let mid = module.self_id();
                    let handle = module.datatype_handle_at(def.enum_handle);
                    let enum_name = module.identifier_at(handle.name);
                    TypeOrigin {
                        module_name: mid.name().to_owned(),
                        type_name: enum_name.to_owned(),
                        origin_id: version_id,
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
        self.accounts
            .insert(stored_package.version_id, stored_package);
    }

    pub fn publish_or_overwrite_module(
        &mut self,
        original_id: OriginalId,
        version_id: DefiningTypeId,
        module_name: Identifier,
        blob: Vec<u8>,
    ) {
        let account = self
            .accounts
            .entry(version_id)
            .or_insert_with(|| StoredPackage::empty(original_id, version_id));
        account.modules.insert(module_name, blob);
    }

    pub fn debug_dump(&self) {
        for (version_id, stored_package) in &self.accounts {
            println!("Version ID: {:?}", version_id);
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

impl<S: ModuleResolver> ModuleResolver for DeltaStorage<'_, '_, S> {
    type Error = S::Error;
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
            .map(|version_id| {
                if let Some(account_storage) = self.delta.accounts().get(version_id) {
                    let module_bytes: BTreeMap<_, _> = account_storage
                        .modules()
                        .iter()
                        .map(|(name, op)| op.clone().ok().map(|blob| (name.clone(), blob)))
                        .collect::<Option<_>>()
                        .unwrap_or_default();

                    Ok(Some(SerializedPackage::raw_package(
                        module_bytes,
                        account_storage.runtime_id(),
                        *version_id,
                    )))
                } else {
                    // TODO: Can optimize this to do a two-pass bulk lookup if we want
                    Ok(self.base.get_packages(&[*version_id])?[0].clone())
                }
            })
            .collect::<Result<Vec<_>, Self::Error>>()
    }
}

impl ModuleResolver for InMemoryStorage {
    type Error = ();

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
            .map(|version_id| {
                if let Some(stored_package) = self.accounts.get(version_id) {
                    Ok(Some(stored_package.clone().into_serialized_package()))
                } else {
                    Ok(None)
                }
            })
            .collect::<Result<Vec<_>, Self::Error>>()
    }
}
