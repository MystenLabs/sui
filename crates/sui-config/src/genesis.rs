// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ValidatorInfo;
use anyhow::Context;
use base64ct::Encoding;
use move_binary_format::CompiledModule;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fs, path::Path};
use sui_types::{
    base_types::TxContext,
    committee::{Committee, EpochId},
    error::SuiResult,
    object::Object,
};
use tracing::{info, trace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Genesis {
    modules: Vec<Vec<CompiledModule>>,
    objects: Vec<Object>,
    genesis_ctx: TxContext,
    validator_set: Vec<ValidatorInfo>,
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

    pub fn epoch(&self) -> EpochId {
        self.genesis_ctx.epoch()
    }

    pub fn validator_set(&self) -> &[ValidatorInfo] {
        &self.validator_set
    }

    pub fn committee(&self) -> SuiResult<Committee> {
        let voting_rights = self
            .validator_set()
            .iter()
            .map(|validator| (validator.public_key(), validator.stake()))
            .collect();
        Committee::new(self.epoch(), voting_rights)
    }

    pub fn get_default_genesis() -> Self {
        Builder::new(sui_adapter::genesis::get_genesis_context()).build()
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, anyhow::Error> {
        let path = path.as_ref();
        trace!("Reading Genesis from {}", path.display());
        let bytes = fs::read(path)
            .with_context(|| format!("Unable to load Genesis from {}", path.display()))?;
        Ok(bcs::from_bytes(&bytes)?)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), anyhow::Error> {
        let path = path.as_ref();
        trace!("Writing Genesis to {}", path.display());
        let bytes = bcs::to_bytes(&self)?;
        fs::write(path, bytes)
            .with_context(|| format!("Unable to save Genesis to {}", path.display()))?;
        Ok(())
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
            validator_set: &'a [ValidatorInfo],
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
            validator_set: &self.validator_set,
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
            validator_set: Vec<ValidatorInfo>,
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
            validator_set: raw_genesis.validator_set,
        })
    }
}

pub struct Builder {
    sui_framework: Option<Vec<CompiledModule>>,
    move_framework: Option<Vec<CompiledModule>>,
    move_modules: Vec<Vec<CompiledModule>>,
    objects: Vec<Object>,
    genesis_ctx: TxContext,
    validators: Vec<ValidatorInfo>,
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

    pub fn add_validator(mut self, validator: ValidatorInfo) -> Self {
        self.validators.push(validator);
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
            validator_set: self.validators,
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
            validator_set: vec![],
        };

        let s = serde_yaml::to_string(&genesis).unwrap();
        let from_s = serde_yaml::from_str(&s).unwrap();
        assert_eq!(genesis, from_s);
    }
}
