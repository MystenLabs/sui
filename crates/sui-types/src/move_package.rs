// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::ObjectID,
    error::{ExecutionError, ExecutionErrorKind, SuiError, SuiResult},
};
use move_binary_format::access::ModuleAccess;
use move_binary_format::binary_views::BinaryIndexedView;
use move_binary_format::file_format::CompiledModule;
use move_binary_format::normalized;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_disassembler::disassembler::Disassembler;
use move_ir_types::location::Spanned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::serde_as;
use serde_with::Bytes;
use std::collections::BTreeMap;
use sui_protocol_constants::*;

// TODO: robust MovePackage tests
// #[cfg(test)]
// #[path = "unit_tests/move_package.rs"]
// mod base_types_tests;

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

// serde_bytes::ByteBuf is an analog of Vec<u8> with built-in fast serialization.
#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct MovePackage {
    id: ObjectID,
    // TODO use session cache
    #[serde_as(as = "BTreeMap<_, Bytes>")]
    module_map: BTreeMap<String, Vec<u8>>,
}

impl MovePackage {
    pub fn new(
        id: ObjectID,
        module_map: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, ExecutionError> {
        let pkg = Self {
            id,
            module_map: module_map.clone(),
        };
        let object_size = pkg.size() as u64;
        if object_size > MAX_MOVE_PACKAGE_SIZE {
            return Err(ExecutionErrorKind::MovePackageTooBig {
                object_size,
                max_object_size: MAX_MOVE_PACKAGE_SIZE,
            }
            .into());
        }
        Ok(pkg)
    }

    pub fn from_module_iter<T: IntoIterator<Item = CompiledModule>>(
        iter: T,
    ) -> Result<Self, ExecutionError> {
        let mut iter = iter.into_iter().peekable();
        let id = ObjectID::from(
            *iter
                .peek()
                .expect("Tried to build a Move package from an empty iterator of Compiled modules")
                .self_id()
                .address(),
        );

        Self::new(
            id,
            &iter
                .map(|module| {
                    let mut bytes = Vec::new();
                    module.serialize(&mut bytes).unwrap();
                    (module.self_id().name().to_string(), bytes)
                })
                .collect(),
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
