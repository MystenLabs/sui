// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use base64ct::Encoding;
use move_binary_format::CompiledModule;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sui_types::{base_types::TxContext, crypto::PublicKeyBytes, object::Object};
use tracing::info;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Genesis {
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

impl Serialize for Genesis {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::Error;

        #[derive(Serialize)]
        struct RawGeneis<'a> {
            modules: Vec<Vec<Vec<u8>>>,
            objects: &'a [Object],
            genesis_ctx: &'a TxContext,
        }

        let mut vec_serialized_modules = Vec::new();
        for modules in &self.modules {
            let mut serialized_modules = Vec::new();
            for module in modules {
                let mut buf = Vec::new();
                module
                    .serialize(&mut buf)
                    .map_err(|e| Error::custom(e.to_string()))?;
                serialized_modules.push(buf);
            }
            vec_serialized_modules.push(serialized_modules);
        }

        let raw_genesis = RawGeneis {
            modules: vec_serialized_modules,
            objects: &self.objects,
            genesis_ctx: &self.genesis_ctx,
        };

        let bytes = bcs::to_bytes(&raw_genesis).map_err(|e| Error::custom(e.to_string()))?;

        if serializer.is_human_readable() {
            let s = base64ct::Base64::encode_string(&bytes);
            serializer.serialize_str(&s)
        } else {
            serializer.serialize_bytes(&bytes)
        }
    }
}

impl<'de> Deserialize<'de> for Genesis {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(Deserialize)]
        struct RawGeneis {
            modules: Vec<Vec<Vec<u8>>>,
            objects: Vec<Object>,
            genesis_ctx: TxContext,
        }

        let bytes = if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            base64ct::Base64::decode_vec(&s).map_err(|e| Error::custom(e.to_string()))?
        } else {
            let data: Vec<u8> = Vec::deserialize(deserializer)?;
            data
        };

        let raw_genesis: RawGeneis =
            bcs::from_bytes(&bytes).map_err(|e| Error::custom(e.to_string()))?;

        let mut modules = Vec::new();
        for serialized_modules in raw_genesis.modules {
            let mut vec_modules = Vec::new();
            for buf in serialized_modules {
                let module =
                    CompiledModule::deserialize(&buf).map_err(|e| Error::custom(e.to_string()))?;
                vec_modules.push(module);
            }
            modules.push(vec_modules);
        }

        Ok(Genesis {
            modules,
            objects: raw_genesis.objects,
            genesis_ctx: raw_genesis.genesis_ctx,
        })
    }
}

pub struct Builder {
    sui_framework: Option<Vec<CompiledModule>>,
    move_framework: Option<Vec<CompiledModule>>,
    move_modules: Vec<Vec<CompiledModule>>,
    objects: Vec<Object>,
    genesis_ctx: TxContext,
    validators: Vec<(PublicKeyBytes, usize)>,
}

impl Builder {
    pub fn new(genesis_ctx: TxContext) -> Self {
        Self {
            sui_framework: None,
            move_framework: None,
            move_modules: vec![],
            objects: vec![],
            genesis_ctx,
            validators: vec![],
        }
    }

    pub fn sui_framework(mut self, sui_framework: Vec<CompiledModule>) -> Self {
        self.sui_framework = Some(sui_framework);
        self
    }

    pub fn move_framework(mut self, move_framework: Vec<CompiledModule>) -> Self {
        self.move_framework = Some(move_framework);
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
        let move_modules = self
            .move_framework
            .unwrap_or_else(sui_framework::get_move_stdlib);
        // let move_framework =
        //     Object::new_package(move_modules.clone(), TransactionDigest::genesis());
        modules.push(move_modules);
        // objects.push(move_framework);

        // Load Sui Framework
        info!("Loading Sui framework lib from {:?}", self.sui_framework);
        let sui_modules = self
            .sui_framework
            .unwrap_or_else(sui_framework::get_sui_framework);
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
    use super::Genesis;

    #[test]
    fn roundtrip() {
        let sui_lib = sui_framework::get_sui_framework();

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
