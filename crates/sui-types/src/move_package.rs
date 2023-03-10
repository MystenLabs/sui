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
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::serde_as;
use serde_with::Bytes;
use std::collections::BTreeMap;

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
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize, Hash)]
pub struct ModuleStruct {
    pub module_name: Identifier,
    pub struct_name: Identifier,
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

    /// maps struct/module to a package version where it was first defined
    type_origin: Option<BTreeMap<ModuleStruct, ObjectID>>,
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
    pub fn new(
        id: ObjectID,
        version: SequenceNumber,
        module_map: &BTreeMap<String, Vec<u8>>,
        max_move_package_size: u64,
    ) -> Result<Self, ExecutionError> {
        Self::new_with_type_origin(id, version, module_map, max_move_package_size, None)
    }

    pub fn new_with_type_origin(
        id: ObjectID,
        version: SequenceNumber,
        module_map: &BTreeMap<String, Vec<u8>>,
        max_move_package_size: u64,
        type_origin: Option<BTreeMap<ModuleStruct, ObjectID>>,
    ) -> Result<Self, ExecutionError> {
        let pkg = Self {
            id,
            version,
            module_map: module_map.clone(),
            type_origin,
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

    pub fn from_module_iter<T: IntoIterator<Item = CompiledModule>>(
        version: SequenceNumber,
        iter: T,
        max_move_package_size: u64,
    ) -> Result<Self, ExecutionError> {
        Self::from_module_iter_with_type_origin(version, iter, max_move_package_size, None)
    }

    pub fn from_module_iter_with_type_origin<T: IntoIterator<Item = CompiledModule>>(
        version: SequenceNumber,
        iter: T,
        max_move_package_size: u64,
        type_origin: Option<BTreeMap<ModuleStruct, ObjectID>>,
    ) -> Result<Self, ExecutionError> {
        let mut iter = iter.into_iter().peekable();
        let id = ObjectID::from(
            *iter
                .peek()
                .expect("Tried to build a Move package from an empty iterator of Compiled modules")
                .self_id()
                .address(),
        );

        Self::new_with_type_origin(
            id,
            version,
            &iter
                .map(|module| {
                    let mut bytes = Vec::new();
                    module.serialize(&mut bytes).unwrap();
                    (module.self_id().name().to_string(), bytes)
                })
                .collect(),
            max_move_package_size,
            type_origin,
        )
    }

    /// Return the size of the package in bytes. Only count the bytes of the modules themselves--the
    /// fact that we store them in a map is an implementation detail
    pub fn size(&self) -> usize {
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
