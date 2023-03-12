// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::{ObjectID, SequenceNumber},
    error::{ExecutionError, ExecutionErrorKind, SuiError, SuiResult},
    id::{ID, UID},
    SUI_FRAMEWORK_ADDRESS,
};
use move_binary_format::access::ModuleAccess;
use move_binary_format::binary_views::BinaryIndexedView;
use move_binary_format::file_format::CompiledModule;
use move_binary_format::normalized;
use move_core_types::{
    account_address::AccountAddress,
    ident_str,
    identifier::{IdentStr, Identifier},
    language_storage::StructTag,
};
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::serde_as;
use serde_with::Bytes;
use std::collections::{BTreeMap, BTreeSet};

// TODO: robust MovePackage tests
// #[cfg(test)]
// #[path = "unit_tests/move_package.rs"]
// mod base_types_tests;

pub const PACKAGE_MODULE_NAME: &IdentStr = ident_str!("package");
pub const UPGRADECAP_STRUCT_NAME: &IdentStr = ident_str!("UpgradeCap");
pub const UPGRADETICKET_STRUCT_NAME: &IdentStr = ident_str!("UpgradeTicket");
pub const UPGRADERECEIPT_STRUCT_NAME: &IdentStr = ident_str!("UpgradeReceipt");

#[derive(Clone, Debug)]
/// Additional information about a function
pub struct FnInfo {
    /// If true, it's a function involved in testing (`[test]`, `[test_only]`, `[expected_failure]`)
    pub is_test: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
/// Uniquely identifies a function in a module
pub struct FnInfoKey {
    pub fn_name: String,
    pub mod_addr: AccountAddress,
}

/// A map from function info keys to function info
pub type FnInfoMap = BTreeMap<FnInfoKey, FnInfo>;

/// Identifies a struct and the module it was defined in
#[derive(
    Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize, Hash, JsonSchema,
)]
pub struct ModuleStruct {
    pub module_name: String,
    pub struct_name: String,
}

/// Upgraded package info for the linkage table
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash, JsonSchema)]
pub struct UpgradeInfo {
    /// ID of the upgraded packages
    upgraded_id: ObjectID,
    /// Version of the upgraded package
    upgraded_version: SequenceNumber,
}

// serde_bytes::ByteBuf is an analog of Vec<u8> with built-in fast serialization.
#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct MovePackage {
    id: ObjectID,
    /// Most move packages are uniquely identified by their ID (i.e. there is only one version per
    /// ID), but the version is still stored because one package may be an upgrade of another (at a
    /// different ID), in which case its version will be one greater than the version of the
    /// upgraded package.
    ///
    /// Framework packages are an exception to this rule -- all versions of the framework packages
    /// exist at the same ID, at increasing versions.
    ///
    /// In all cases, packages are referred to by move calls using just their ID, and they are
    /// always loaded at their latest version.
    version: SequenceNumber,
    // TODO use session cache
    #[serde_as(as = "BTreeMap<_, Bytes>")]
    module_map: BTreeMap<String, Vec<u8>>,

    /// Maps struct/module to a package version where it was first defined
    type_origin_table: BTreeMap<ModuleStruct, ObjectID>,

    // For each dependency, maps original package ID to the info about the (upgraded) dependency
    // version that this package is using
    linkage_table: BTreeMap<ObjectID, UpgradeInfo>,
}

/// Rust representation of upgrade policy constants in `sui::package`.
#[allow(dead_code)]
const UPGRADE_POLICY_COMPATIBLE: u8 = 0;
#[allow(dead_code)]
const UPGRADE_POLICY_ADDITIVE: u8 = 128;
#[allow(dead_code)]
const UPGRADE_POLICY_DEP_ONLY: u8 = 192;

/// Rust representation of `sui::package::UpgradeCap`.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpgradeCap {
    pub id: UID,
    pub package: ID,
    pub version: u64,
    pub policy: u8,
}

/// Rust representation of `sui::package::UpgradeTicket`.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpgradeTicket {
    pub cap: ID,
    pub package: ID,
    pub policy: u8,
    pub digest: Vec<u8>,
}

/// Rust representation of `sui::package::UpgradeReceipt`.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpgradeReceipt {
    pub cap: ID,
    pub package: ID,
}

impl MovePackage {
    /// Create a package with all required data (including serialized modules, type origin and
    /// linkage tables) already supplied.
    pub fn new(
        id: ObjectID,
        version: SequenceNumber,
        module_map: BTreeMap<String, Vec<u8>>,
        max_move_package_size: u64,
        type_origin_table: BTreeMap<ModuleStruct, ObjectID>,
        linkage_table: BTreeMap<ObjectID, UpgradeInfo>,
    ) -> Result<Self, ExecutionError> {
        let pkg = Self {
            id,
            version,
            module_map,
            type_origin_table,
            linkage_table,
        };
        let object_size = pkg.size() as u64;
        if object_size > max_move_package_size {
            return Err(ExecutionErrorKind::MovePackageTooBig {
                object_size,
                max_object_size: max_move_package_size,
            }
            .into());
        }
        Ok(pkg)
    }

    /// Create an initial version of the package along with this version's type origin and linkage
    /// tables.
    pub fn new_initial<'p>(
        version: SequenceNumber,
        modules: Vec<CompiledModule>,
        max_move_package_size: u64,
        transitive_dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<Self, ExecutionError> {
        let module = modules
            .first()
            .expect("Tried to build a Move package from an empty iterator of Compiled modules");
        let self_id = ObjectID::from(*module.address());
        let storage_id = self_id;
        let type_origin_table = build_initial_type_origin_table(&modules);
        Self::from_module_iter_with_type_origin_table(
            storage_id,
            self_id,
            version,
            modules,
            max_move_package_size,
            type_origin_table,
            transitive_dependencies,
        )
    }

    /// Create an upgraded version of the package along with this version's type origin and linkage
    /// tables.
    pub fn new_upgraded<'p>(
        &self,
        storage_id: ObjectID,
        modules: Vec<CompiledModule>,
        max_move_package_size: u64,
        transitive_dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<Self, ExecutionError> {
        let module = modules
            .first()
            .expect("Tried to build a Move package from an empty iterator of Compiled modules");
        let self_id = ObjectID::from(*module.address());
        let type_origin_table = build_upgraded_type_origin_table(self, &modules);
        let mut new_version = self.version();
        new_version.increment();
        Self::from_module_iter_with_type_origin_table(
            storage_id,
            self_id,
            new_version,
            modules,
            max_move_package_size,
            type_origin_table,
            transitive_dependencies,
        )
    }

    fn from_module_iter_with_type_origin_table<'p>(
        storage_id: ObjectID,
        self_id: ObjectID,
        version: SequenceNumber,
        modules: impl IntoIterator<Item = CompiledModule>,
        max_move_package_size: u64,
        type_origin_table: BTreeMap<ModuleStruct, ObjectID>,
        transitive_dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<Self, ExecutionError> {
        let mut module_map = BTreeMap::new();
        let mut immediate_dependencies = BTreeSet::new();

        for module in modules {
            let name = module.name().to_string();

            immediate_dependencies.extend(
                module
                    .immediate_dependencies()
                    .into_iter()
                    .map(|dep| ObjectID::from(*dep.address())),
            );

            let mut bytes = Vec::new();
            module.serialize(&mut bytes).unwrap();
            module_map.insert(name, bytes);
        }

        immediate_dependencies.remove(&self_id);
        let linkage_table = build_linkage_table(immediate_dependencies, transitive_dependencies)?;
        Self::new(
            storage_id,
            version,
            module_map,
            max_move_package_size,
            type_origin_table,
            linkage_table,
        )
    }

    /// Return the size of the package in bytes. Only count the bytes of the modules themselves--the
    /// fact that we store them in a map is an implementation detail
    pub fn size(&self) -> usize {
        // TODO: Add sizes of linkage and type origin tables
        self.module_map.values().map(|b| b.len()).sum()
    }

    pub fn id(&self) -> ObjectID {
        self.id
    }

    pub fn version(&self) -> SequenceNumber {
        self.version
    }

    pub fn increment_version(&mut self) {
        self.version.increment();
    }

    /// Approximate size of the package in bytes. This is used for gas metering.
    pub fn object_size_for_gas_metering(&self) -> usize {
        // + 8 for version
        self.serialized_module_map()
            .iter()
            .map(|(name, module)| name.len() + module.len())
            .sum::<usize>()
            + 8
    }

    pub fn serialized_module_map(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.module_map
    }

    pub fn type_origin_table(&self) -> &BTreeMap<ModuleStruct, ObjectID> {
        &self.type_origin_table
    }

    pub fn linkage_table(&self) -> &BTreeMap<ObjectID, UpgradeInfo> {
        &self.linkage_table
    }

    /// The ObjectID that this package's modules believe they are from, at runtime (can differ from
    /// `MovePackage::id()` in the case of package upgrades).
    pub fn original_package_id(&self) -> ObjectID {
        let bytes = self.module_map.values().next().expect("Empty module map");
        let module = CompiledModule::deserialize(bytes)
            .expect("A Move package contains a module that cannot be deserialized");
        (*module.address()).into()
    }

    pub fn deserialize_module(&self, module: &Identifier) -> SuiResult<CompiledModule> {
        // TODO use the session's cache
        let bytes = self
            .serialized_module_map()
            .get(module.as_str())
            .ok_or_else(|| SuiError::ModuleNotFound {
                module_name: module.to_string(),
            })?;
        Ok(CompiledModule::deserialize(bytes)
            .expect("Unwrap safe because Sui serializes/verifies modules before publishing them"))
    }

    pub fn disassemble(&self) -> SuiResult<BTreeMap<String, Value>> {
        disassemble_modules(self.module_map.values())
    }

    pub fn normalize(&self) -> SuiResult<BTreeMap<String, normalized::Module>> {
        normalize_modules(self.module_map.values())
    }
}

impl UpgradeCap {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: PACKAGE_MODULE_NAME.to_owned(),
            name: UPGRADECAP_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }

    /// Create an `UpgradeCap` for the newly published package at `package_id`, and associate it with
    /// the fresh `uid`.
    pub fn new(uid: ObjectID, package_id: ObjectID) -> Self {
        UpgradeCap {
            id: UID::new(uid),
            package: ID { bytes: package_id },
            version: 1,
            policy: UPGRADE_POLICY_COMPATIBLE,
        }
    }
}

impl UpgradeTicket {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: PACKAGE_MODULE_NAME.to_owned(),
            name: UPGRADETICKET_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }
}

impl UpgradeReceipt {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: PACKAGE_MODULE_NAME.to_owned(),
            name: UPGRADERECEIPT_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }

    /// Create an `UpgradeReceipt` for the upgraded package at `package_id` using the
    /// `UpgradeTicket` and newly published package id.
    pub fn new(upgrade_ticket: UpgradeTicket, upgraded_package_id: ObjectID) -> Self {
        UpgradeReceipt {
            cap: upgrade_ticket.cap,
            package: ID {
                bytes: upgraded_package_id,
            },
        }
    }
}

pub fn disassemble_modules<'a, I>(modules: I) -> SuiResult<BTreeMap<String, Value>>
where
    I: Iterator<Item = &'a Vec<u8>>,
{
    let mut disassembled = BTreeMap::new();
    for bytecode in modules {
        let module = CompiledModule::deserialize(bytecode).map_err(|error| {
            SuiError::ModuleDeserializationFailure {
                error: error.to_string(),
            }
        })?;
        let view = BinaryIndexedView::Module(&module);
        let d = Disassembler::from_view(view, Spanned::unsafe_no_loc(()).loc).map_err(|e| {
            SuiError::ObjectSerializationError {
                error: e.to_string(),
            }
        })?;
        let bytecode_str = d
            .disassemble()
            .map_err(|e| SuiError::ObjectSerializationError {
                error: e.to_string(),
            })?;
        disassembled.insert(module.name().to_string(), Value::String(bytecode_str));
    }
    Ok(disassembled)
}

pub fn normalize_modules<'a, I>(modules: I) -> SuiResult<BTreeMap<String, normalized::Module>>
where
    I: Iterator<Item = &'a Vec<u8>>,
{
    let mut normalized_modules = BTreeMap::new();
    for bytecode in modules {
        let module = CompiledModule::deserialize(bytecode).map_err(|error| {
            SuiError::ModuleDeserializationFailure {
                error: error.to_string(),
            }
        })?;
        let normalized_module = normalized::Module::new(&module);
        normalized_modules.insert(normalized_module.name.to_string(), normalized_module);
    }
    Ok(normalized_modules)
}

pub fn normalize_deserialized_modules<'a, I>(modules: I) -> BTreeMap<String, normalized::Module>
where
    I: Iterator<Item = &'a CompiledModule>,
{
    let mut normalized_modules = BTreeMap::new();
    for module in modules {
        let normalized_module = normalized::Module::new(module);
        normalized_modules.insert(normalized_module.name.to_string(), normalized_module);
    }
    normalized_modules
}

fn build_linkage_table<'p>(
    mut immediate_dependencies: BTreeSet<ObjectID>,
    transitive_dependencies: impl IntoIterator<Item = &'p MovePackage>,
) -> Result<BTreeMap<ObjectID, UpgradeInfo>, ExecutionError> {
    let mut linkage_table = BTreeMap::new();
    let mut dep_linkage_tables = vec![];

    for transitive_dep in transitive_dependencies.into_iter() {
        let original_id = transitive_dep.original_package_id();

        if immediate_dependencies.remove(&original_id) {
            // Found an immediate dependency, mark it as seen, and stash a reference to its linkage
            // table to check later.
            dep_linkage_tables.push(&transitive_dep.linkage_table);
        }

        linkage_table.insert(
            original_id,
            UpgradeInfo {
                upgraded_id: transitive_dep.id,
                upgraded_version: transitive_dep.version,
            },
        );
    }
    // (1) Every dependency is represented in the transitive dependencies
    if !immediate_dependencies.is_empty() {
        // TODO ExecutionError
        panic!("Dependency not found in linkage table");
    }

    // (2) Every dependency's linkage table is superseded by this linkage table
    for dep_linkage_table in dep_linkage_tables {
        for (original_id, dep_info) in dep_linkage_table {
            let Some(our_info) = linkage_table.get(original_id) else {
                // TODO ExecutionError
                panic!("Transitive dependency not found in linkage table");
            };

            if our_info.upgraded_version < dep_info.upgraded_version {
                // TODO ExecutionError
                panic!("Downgrade in linkage table");
            }
        }
    }

    Ok(linkage_table)
}

fn build_initial_type_origin_table(modules: &[CompiledModule]) -> BTreeMap<ModuleStruct, ObjectID> {
    BTreeMap::from_iter(modules.iter().flat_map(|m| {
        m.struct_defs().iter().map(|struct_def| {
            let struct_handle = m.struct_handle_at(struct_def.struct_handle);
            let module_name = m.name().to_string();
            let struct_name = m.identifier_at(struct_handle.name).to_string();
            let id: ObjectID = (*m.self_id().address()).into();
            (
                ModuleStruct {
                    module_name,
                    struct_name,
                },
                id,
            )
        })
    }))
}

fn build_upgraded_type_origin_table(
    predecessor: &MovePackage,
    modules: &[CompiledModule],
) -> BTreeMap<ModuleStruct, ObjectID> {
    let mut new_table = predecessor.type_origin_table.clone();
    for m in modules {
        for struct_def in m.struct_defs() {
            let struct_handle = m.struct_handle_at(struct_def.struct_handle);
            let module_name = m.name().to_string();
            let struct_name = m.identifier_at(struct_handle.name).to_string();
            let mod_struct = ModuleStruct {
                module_name,
                struct_name,
            };
            // only insert types that are not in the original table as only these should be
            // marked as originating from the current package
            if predecessor.type_origin_table.contains_key(&mod_struct) {
                continue;
            }
            let id: ObjectID = (*m.self_id().address()).into();
            new_table.insert(mod_struct, id);
        }
    }
    new_table
}
