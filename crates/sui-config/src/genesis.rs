// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ValidatorInfo;
use anyhow::Context;
use anyhow::Result;
use move_binary_format::CompiledModule;
use move_core_types::ident_str;
use move_core_types::language_storage::ModuleId;
use move_vm_runtime::native_functions::NativeFunctionTable;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;
use std::{fs, path::Path};
use sui_adapter::adapter;
use sui_adapter::adapter::MoveVM;
use sui_adapter::in_memory_storage::InMemoryStorage;
use sui_adapter::temporary_store::TemporaryStore;
use sui_types::base_types::ObjectID;
use sui_types::base_types::TransactionDigest;
use sui_types::gas::SuiGasStatus;
use sui_types::messages::CallArg;
use sui_types::messages::InputObjects;
use sui_types::messages::Transaction;
use sui_types::sui_serde::{Base64, Encoding};
use sui_types::MOVE_STDLIB_ADDRESS;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::{
    base_types::TxContext,
    committee::{Committee, EpochId},
    error::SuiResult,
    object::Object,
};
use tracing::{info, trace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Genesis {
    objects: Vec<Object>,
    validator_set: Vec<ValidatorInfo>,
}

impl Genesis {
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    pub fn epoch(&self) -> EpochId {
        0
    }

    pub fn validator_set(&self) -> &[ValidatorInfo] {
        &self.validator_set
    }

    pub fn committee(&self) -> SuiResult<Committee> {
        Committee::new(
            self.epoch(),
            ValidatorInfo::voting_rights(self.validator_set()),
        )
    }

    pub fn get_default_genesis() -> Self {
        Builder::new_with_context(sui_adapter::genesis::get_genesis_context()).build()
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
            objects: &'a [Object],
            validator_set: &'a [ValidatorInfo],
        }

        let raw_genesis = RawGeneis {
            objects: &self.objects,
            validator_set: &self.validator_set,
        };

        let bytes = bcs::to_bytes(&raw_genesis).map_err(|e| Error::custom(e.to_string()))?;

        if serializer.is_human_readable() {
            let s = Base64::encode(&bytes);
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
            objects: Vec<Object>,
            validator_set: Vec<ValidatorInfo>,
        }

        let bytes = if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            Base64::decode(&s).map_err(|e| Error::custom(e.to_string()))?
        } else {
            let data: Vec<u8> = Vec::deserialize(deserializer)?;
            data
        };

        let raw_genesis: RawGeneis =
            bcs::from_bytes(&bytes).map_err(|e| Error::custom(e.to_string()))?;

        Ok(Genesis {
            objects: raw_genesis.objects,
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

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        let genesis_ctx = sui_adapter::genesis::get_genesis_context();
        Self::new_with_context(genesis_ctx)
    }

    pub fn new_with_context(genesis_ctx: TxContext) -> Self {
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

    pub fn add_validator(mut self, validator: ValidatorInfo) -> Self {
        self.validators.push(validator);
        self
    }

    pub fn build(self) -> Genesis {
        let mut modules = Vec::new();
        let objects = self.objects;
        let mut genesis_ctx = self.genesis_ctx;

        // Load Move Framework
        info!("Loading Move framework lib from {:?}", self.move_framework);
        let move_modules = self
            .move_framework
            .unwrap_or_else(sui_framework::get_move_stdlib);
        modules.push(move_modules);

        // Load Sui Framework
        info!("Loading Sui framework lib from {:?}", self.sui_framework);
        let sui_modules = self
            .sui_framework
            .unwrap_or_else(sui_framework::get_sui_framework);
        modules.push(sui_modules);

        // add custom modules
        modules.extend(self.move_modules);

        let objects =
            create_genesis_objects(&mut genesis_ctx, &modules, &objects, &self.validators);

        Genesis {
            objects,
            validator_set: self.validators,
        }
    }
}

fn create_genesis_objects(
    genesis_ctx: &mut TxContext,
    modules: &[Vec<CompiledModule>],
    input_objects: &[Object],
    validators: &[ValidatorInfo],
) -> Vec<Object> {
    let mut store = InMemoryStorage::new(Vec::new());

    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let move_vm = adapter::new_move_vm(native_functions.clone())
        .expect("We defined natives to not fail here");

    for modules in modules {
        process_package(
            &mut store,
            &native_functions,
            genesis_ctx,
            modules.to_owned(),
        )
        .unwrap();
    }

    for object in input_objects {
        store.insert_object(object.to_owned());
    }

    generate_genesis_system_object(&mut store, &move_vm, validators, genesis_ctx).unwrap();

    store
        .into_inner()
        .into_iter()
        .map(|(_id, object)| object)
        .collect()
}

fn process_package(
    store: &mut InMemoryStorage,
    // mv: &MoveVM,
    native_functions: &NativeFunctionTable,
    ctx: &mut TxContext,
    modules: Vec<CompiledModule>,
) -> Result<()> {
    let inputs = Transaction::input_objects_in_compiled_modules(&modules);
    let ids: Vec<_> = inputs.iter().map(|kind| kind.object_id()).collect();
    let input_objects = store.get_objects(&ids[..]);
    // When publishing genesis packages, since the std framework packages all have
    // non-zero addresses, [`Transaction::input_objects_in_compiled_modules`] will consider
    // them as dependencies even though they are not. Hence input_objects contain objects
    // that don't exist on-chain because they are yet to be published.
    #[cfg(debug_assertions)]
    {
        let to_be_published_addresses: HashSet<_> = modules
            .iter()
            .map(|module| *module.self_id().address())
            .collect();
        assert!(
            // An object either exists on-chain, or is one of the packages to be published.
            inputs
                .iter()
                .zip(input_objects.iter())
                .all(|(kind, obj_opt)| obj_opt.is_some()
                    || to_be_published_addresses.contains(&kind.object_id()))
        );
    }
    let filtered = inputs
        .into_iter()
        .zip(input_objects.into_iter())
        .filter_map(|(input, object_opt)| object_opt.map(|object| (input, object.to_owned())))
        .collect::<Vec<_>>();

    debug_assert!(ctx.digest() == TransactionDigest::genesis());
    let mut temporary_store =
        TemporaryStore::new(&store, InputObjects::new(filtered), ctx.digest());
    let package_id = ObjectID::from(*modules[0].self_id().address());
    let natives = native_functions.clone();
    let mut gas_status = SuiGasStatus::new_unmetered();
    let vm = adapter::verify_and_link(
        &temporary_store,
        &modules,
        package_id,
        natives,
        &mut gas_status,
    )?;
    adapter::store_package_and_init_modules(
        &mut temporary_store,
        &vm,
        modules,
        ctx,
        &mut gas_status,
    )?;

    let (_objects, _mutable_inputs, written, deleted, _events) = temporary_store.into_inner();

    store.finish(written, deleted);

    Ok(())
}

pub fn generate_genesis_system_object(
    store: &mut InMemoryStorage,
    move_vm: &MoveVM,
    committee: &[ValidatorInfo],
    genesis_ctx: &mut TxContext,
) -> Result<()> {
    let genesis_digest = genesis_ctx.digest();
    let mut temporary_store =
        TemporaryStore::new(&store, InputObjects::new(vec![]), genesis_digest);

    let mut pubkeys = Vec::new();
    let mut sui_addresses = Vec::new();
    let mut network_addresses = Vec::new();
    let mut names = Vec::new();
    let mut stakes = Vec::new();

    for validator in committee {
        pubkeys.push(validator.public_key());
        sui_addresses.push(validator.sui_address());
        network_addresses.push(validator.network_address());
        names.push(validator.name().to_owned().into_bytes());
        stakes.push(validator.stake());
    }

    adapter::execute(
        move_vm,
        &mut temporary_store,
        ModuleId::new(SUI_FRAMEWORK_ADDRESS, ident_str!("genesis").to_owned()),
        &ident_str!("create").to_owned(),
        vec![],
        vec![
            CallArg::Pure(bcs::to_bytes(&pubkeys).unwrap()),
            CallArg::Pure(bcs::to_bytes(&sui_addresses).unwrap()),
            CallArg::Pure(bcs::to_bytes(&names).unwrap()),
            CallArg::Pure(bcs::to_bytes(&network_addresses).unwrap()),
            CallArg::Pure(bcs::to_bytes(&stakes).unwrap()),
        ],
        &mut SuiGasStatus::new_unmetered(),
        genesis_ctx,
    )?;

    let (_objects, _mutable_inputs, written, deleted, _events) = temporary_store.into_inner();

    store.finish(written, deleted);

    Ok(())
}

#[cfg(test)]
mod test {
    use super::Builder;

    #[test]
    fn roundtrip() {
        let genesis =
            Builder::new_with_context(sui_adapter::genesis::get_genesis_context()).build();

        let s = serde_yaml::to_string(&genesis).unwrap();
        let from_s = serde_yaml::from_str(&s).unwrap();
        assert_eq!(genesis, from_s);
    }
}
