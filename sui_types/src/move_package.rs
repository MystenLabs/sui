// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::readable_serde::encoding::Base64;
use crate::readable_serde::Readable;
use crate::{
    base_types::ObjectID,
    error::{SuiError, SuiResult},
};
use move_binary_format::file_format::CompiledModule;
use move_core_types::identifier::Identifier;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serde_with::Bytes;
use std::collections::BTreeMap;

// TODO: robust MovePackage tests
// #[cfg(test)]
// #[path = "unit_tests/move_package.rs"]
// mod base_types_tests;

// serde_bytes::ByteBuf is an analog of Vec<u8> with built-in fast serialization.
#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash, JsonSchema)]
pub struct MovePackage {
    id: ObjectID,
    // TODO use session cache
    #[schemars(with = "BTreeMap<String, String>")]
    #[serde_as(as = "BTreeMap<_, Readable<Base64, Bytes>>")]
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
