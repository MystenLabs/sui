// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::ObjectID,
    error::{SuiError, SuiResult},
};
use move_binary_format::access::ModuleAccess;
use move_binary_format::binary_views::BinaryIndexedView;
use move_binary_format::file_format::CompiledModule;
use move_core_types::identifier::Identifier;
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
    pub fn new(id: ObjectID, module_map: &BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            id,
            module_map: module_map.clone(),
        }
    }

    pub fn id(&self) -> ObjectID {
        self.id
    }

    pub fn serialized_module_map(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.module_map
    }

    pub fn modules(&self) -> BTreeMap<Identifier, CompiledModule> {
        let mut modules = BTreeMap::new();
        for (name, bytes) in &self.module_map {
            modules.insert(
                Identifier::new(name.clone())
                    .expect("A well-formed module map should contain valid module identifiers"),
                CompiledModule::deserialize(bytes).expect(
                    "Unwrap safe because Sui serializes/verifies modules before publishing them",
                ),
            );
        }
        modules
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

    pub fn into(self) -> (ObjectID, BTreeMap<String, Vec<u8>>) {
        (self.id, self.module_map)
    }
}

pub fn disassemble_modules<'a, I>(modules: I) -> SuiResult<BTreeMap<String, Value>>
where
    I: Iterator<Item = &'a Vec<u8>>,
{
    let mut disassembled = BTreeMap::new();
    for bytecode in modules {
        let module = CompiledModule::deserialize(bytecode)
            .expect("Adapter publish flow ensures that this bytecode deserializes");
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

impl FromIterator<CompiledModule> for MovePackage {
    fn from_iter<T: IntoIterator<Item = CompiledModule>>(iter: T) -> Self {
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
}
