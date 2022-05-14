// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use base64ct::Encoding;
use move_binary_format::CompiledModule;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{serde_as, DeserializeAs, SerializeAs};
use sui_types::{base_types::TransactionDigest, crypto::PublicKeyBytes, object::Object};
use tracing::info;

#[serde_as]
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Genesis {
    #[serde_as(as = "Vec<SerdeCompiledModule>")]
    modules: Vec<CompiledModule>,
    objects: Vec<Object>,
}

impl Genesis {
    pub fn modules(&self) -> &[CompiledModule] {
        &self.modules
    }

    pub fn objects(&self) -> &[Object] {
        &self.objects
    }
}

struct SerdeCompiledModule;

impl SerializeAs<CompiledModule> for SerdeCompiledModule {
    fn serialize_as<S>(module: &CompiledModule, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::Error;

        let mut serialized_module = Vec::new();
        module
            .serialize(&mut serialized_module)
            .map_err(|e| Error::custom(e.to_string()))?;

        if serializer.is_human_readable() {
            let s = base64ct::Base64::encode_string(serialized_module.as_ref());
            s.serialize(serializer)
        } else {
            serialized_module.serialize(serializer)
        }
    }
}

impl<'de> DeserializeAs<'de, CompiledModule> for SerdeCompiledModule {
    fn deserialize_as<D>(deserializer: D) -> Result<CompiledModule, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let data =
                base64ct::Base64::decode_vec(&s).map_err(|e| Error::custom(e.to_string()))?;
            CompiledModule::deserialize(&data).map_err(|e| Error::custom(e.to_string()))
        } else {
            let data: Vec<u8> = Vec::deserialize(deserializer)?;
            CompiledModule::deserialize(&data).map_err(|e| Error::custom(e.to_string()))
        }
    }
}

#[derive(Default)]
pub struct Builder {
    sui_framework: Option<PathBuf>,
    move_framework: Option<PathBuf>,
    move_modules: Vec<PathBuf>,
    validators: Vec<(PublicKeyBytes, usize)>,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sui_framework(mut self, path: PathBuf) -> Self {
        self.sui_framework = Some(path);
        self
    }

    pub fn move_framework(mut self, path: PathBuf) -> Self {
        self.move_framework = Some(path);
        self
    }

    pub fn add_move_module(mut self, path: PathBuf) -> Self {
        self.move_modules.push(path);
        self
    }

    // pub fn add_account(mut self, config: AccountConfig) -> Self {
    //     self.accounts.push(config);
    //     self
    // }

    pub fn add_validator(mut self, public_key: PublicKeyBytes, stake: usize) -> Self {
        self.validators.push((public_key, stake));
        self
    }

    pub fn build(self) -> Genesis {
        let mut modules = Vec::new();
        let mut objects = Vec::new();

        // Load Sui Framework
        let sui_framework_lib_path = self.sui_framework.unwrap();
        info!(
            "Loading Sui framework lib from {:?}",
            sui_framework_lib_path
        );
        let sui_modules =
            sui_framework::get_sui_framework_modules(&sui_framework_lib_path).unwrap();
        // let sui_framework = Object::new_package(sui_modules.clone(), TransactionDigest::genesis());
        modules.extend(sui_modules);
        // objects.push(sui_framework);

        // Load Move Framework
        let move_framework_lib_path = self.move_framework.unwrap();
        info!(
            "Loading Move framework lib from {:?}",
            move_framework_lib_path
        );
        let move_modules =
            sui_framework::get_move_stdlib_modules(&move_framework_lib_path).unwrap();
        // let move_framework =
        //     Object::new_package(move_modules.clone(), TransactionDigest::genesis());
        modules.extend(move_modules);
        // objects.push(move_framework);

        Genesis { modules, objects }
    }
}

#[cfg(test)]
mod test {
    use sui_framework::DEFAULT_FRAMEWORK_PATH;

    use super::Genesis;

    #[test]
    fn roundtrip() {
        let sui_lib =
            sui_framework::get_sui_framework_modules(DEFAULT_FRAMEWORK_PATH.as_ref()).unwrap();

        let genesis = Genesis {
            modules: sui_lib,
            objects: vec![],
        };

        let s = serde_json::to_string_pretty(&genesis).unwrap();
        let from_s = serde_json::from_str(&s).unwrap();
        assert_eq!(genesis, from_s);
    }
}
