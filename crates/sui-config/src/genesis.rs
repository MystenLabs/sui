// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use base64ct::Encoding;
use move_binary_format::CompiledModule;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{serde_as, DeserializeAs, SerializeAs};
use std::path::PathBuf;
use sui_framework::DEFAULT_FRAMEWORK_PATH;
use sui_types::{base_types::TxContext, crypto::PublicKeyBytes, object::Object};
use tracing::info;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Genesis {
    #[serde_as(as = "Vec<Vec<SerdeCompiledModule>>")]
    modules: Vec<Vec<CompiledModule>>,
    objects: Vec<Object>,
    genesis_ctx: TxContext,
}

impl Genesis {
    pub fn modules(&self) -> &[Vec<CompiledModule>] {
        &self.modules
    }

    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    pub fn genesis_ctx(&self) -> &TxContext {
        &self.genesis_ctx
    }

    pub fn get_default_genesis() -> Self {
        Builder::new(sui_adapter::genesis::get_genesis_context()).build()
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

pub struct Builder {
    sui_framework: PathBuf,
    move_framework: PathBuf,
    move_modules: Vec<Vec<CompiledModule>>,
    objects: Vec<Object>,
    genesis_ctx: TxContext,
    validators: Vec<(PublicKeyBytes, usize)>,
}

impl Builder {
    pub fn new(genesis_ctx: TxContext) -> Self {
        Self {
            sui_framework: PathBuf::from(DEFAULT_FRAMEWORK_PATH),
            move_framework: PathBuf::from(DEFAULT_FRAMEWORK_PATH)
                .join("deps")
                .join("move-stdlib"),
            move_modules: vec![],
            objects: vec![],
            genesis_ctx,
            validators: vec![],
        }
    }

    pub fn sui_framework(mut self, path: PathBuf) -> Self {
        self.sui_framework = path;
        self
    }

    pub fn move_framework(mut self, path: PathBuf) -> Self {
        self.move_framework = path;
        self
    }

    pub fn add_move_modules(mut self, modules: Vec<Vec<CompiledModule>>) -> Self {
        self.move_modules = modules;
        self
    }

    pub fn add_object(mut self, object: Object) -> Self {
        self.objects.push(object);
        self
    }

    pub fn add_objects(mut self, objects: Vec<Object>) -> Self {
        self.objects.extend(objects);
        self
    }

    // pub fn add_account(mut self, config: AccountConfig) -> Self {
    //     self.accounts.push(config);
    //     self
    // }

    //TODO actually use the validators added to genesis
    pub fn add_validator(mut self, public_key: PublicKeyBytes, stake: usize) -> Self {
        self.validators.push((public_key, stake));
        self
    }

    pub fn build(self) -> Genesis {
        let mut modules = Vec::new();
        let objects = self.objects;

        // Load Move Framework
        info!("Loading Move framework lib from {:?}", self.move_framework);
        let move_modules = sui_framework::get_move_stdlib_modules(&self.move_framework).unwrap();
        // let move_framework =
        //     Object::new_package(move_modules.clone(), TransactionDigest::genesis());
        modules.push(move_modules);
        // objects.push(move_framework);

        // Load Sui Framework
        info!("Loading Sui framework lib from {:?}", self.sui_framework);
        let sui_modules = sui_framework::get_sui_framework_modules(&self.sui_framework).unwrap();
        // let sui_framework = Object::new_package(sui_modules.clone(), TransactionDigest::genesis());
        modules.push(sui_modules);
        // objects.push(sui_framework);

        // add custom modules
        modules.extend(self.move_modules);

        Genesis {
            modules,
            objects,
            genesis_ctx: self.genesis_ctx,
        }
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
            modules: vec![sui_lib],
            objects: vec![],
            genesis_ctx: sui_adapter::genesis::get_genesis_context(),
        };

        let s = serde_json::to_string_pretty(&genesis).unwrap();
        let from_s = serde_json::from_str(&s).unwrap();
        assert_eq!(genesis, from_s);
    }
}
