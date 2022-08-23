// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ValidatorInfo;
use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use move_binary_format::CompiledModule;
use move_core_types::ident_str;
use move_core_types::language_storage::ModuleId;
use move_vm_runtime::native_functions::NativeFunctionTable;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::{fs, path::Path};
use sui_adapter::adapter;
use sui_adapter::adapter::MoveVM;
use sui_adapter::in_memory_storage::InMemoryStorage;
use sui_adapter::temporary_store::{InnerTemporaryStore, TemporaryStore};
use sui_types::base_types::ObjectID;
use sui_types::base_types::TransactionDigest;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::gas::SuiGasStatus;
use sui_types::messages::CallArg;
use sui_types::messages::InputObjects;
use sui_types::messages::Transaction;
use sui_types::sui_serde::{Base64, Encoding};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::MOVE_STDLIB_ADDRESS;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::{
    base_types::{encode_bytes_hex, TxContext},
    committee::{Committee, EpochId},
    error::SuiResult,
    object::Object,
};
use tracing::trace;

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

    pub fn narwhal_committee(&self) -> narwhal_config::SharedCommittee {
        let narwhal_committee = self
            .validator_set
            .iter()
            .map(|validator| {
                let name = validator
                    .public_key()
                    .try_into()
                    .expect("Can't get narwhal public key");
                let primary = narwhal_config::PrimaryAddresses {
                    primary_to_primary: validator.narwhal_primary_to_primary.clone(),
                    worker_to_primary: validator.narwhal_worker_to_primary.clone(),
                };
                let workers = [(
                    0, // worker_id
                    narwhal_config::WorkerInfo {
                        primary_to_worker: validator.narwhal_primary_to_worker.clone(),
                        transactions: validator.narwhal_consensus_address.clone(),
                        worker_to_worker: validator.narwhal_worker_to_worker.clone(),
                    },
                )]
                .into_iter()
                .collect();
                let authority = narwhal_config::Authority {
                    stake: validator.stake as narwhal_config::Stake, //TODO this should at least be the same size integer
                    primary,
                    workers,
                };

                (name, authority)
            })
            .collect();
        std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(narwhal_config::Committee {
            authorities: narwhal_committee,
            epoch: self.epoch() as narwhal_config::Epoch,
        }))
    }

    pub fn sui_system_object(&self) -> SuiSystemState {
        let sui_system_object = self
            .objects()
            .iter()
            .find(|o| o.id() == sui_types::SUI_SYSTEM_STATE_OBJECT_ID)
            .expect("Sui System State object must always exist");
        let move_object = sui_system_object
            .data
            .try_as_move()
            .expect("Sui System State object must be a Move object");
        let result = bcs::from_bytes::<SuiSystemState>(move_object.contents())
            .expect("Sui System State object deserialization cannot fail");
        result
    }

    pub fn get_default_genesis() -> Self {
        Builder::new().build()
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, anyhow::Error> {
        let path = path.as_ref();
        trace!("Reading Genesis from {}", path.display());
        let bytes = fs::read(path)
            .with_context(|| format!("Unable to load Genesis from {}", path.display()))?;
        bcs::from_bytes(&bytes)
            .with_context(|| format!("Unable to parse Genesis from {}", path.display()))
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), anyhow::Error> {
        let path = path.as_ref();
        trace!("Writing Genesis to {}", path.display());
        let bytes = bcs::to_bytes(&self)?;
        fs::write(path, bytes)
            .with_context(|| format!("Unable to save Genesis to {}", path.display()))?;
        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(self).expect("failed to serialize genesis")
    }

    pub fn sha3(&self) -> [u8; 32] {
        use digest::Digest;
        use std::io::Write;

        let mut digest = sha3::Sha3_256::default();
        digest.write_all(&self.to_bytes()).unwrap();
        let hash = digest.finalize();
        hash.into()
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
    objects: BTreeMap<ObjectID, Object>,
    validators: BTreeMap<AuthorityPublicKeyBytes, ValidatorInfo>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self {
            objects: Default::default(),
            validators: Default::default(),
        }
    }

    pub fn add_object(mut self, object: Object) -> Self {
        self.objects.insert(object.id(), object);
        self
    }

    pub fn add_objects(mut self, objects: Vec<Object>) -> Self {
        for object in objects {
            self.objects.insert(object.id(), object);
        }
        self
    }

    pub fn add_validator(mut self, validator: ValidatorInfo) -> Self {
        self.validators.insert(validator.public_key(), validator);
        self
    }

    pub fn build(self) -> Genesis {
        let mut genesis_ctx = sui_adapter::genesis::get_genesis_context();

        // Get Move and Sui Framework
        let modules = [
            sui_framework::get_move_stdlib(),
            sui_framework::get_sui_framework(),
        ];

        let objects = self.objects.into_iter().map(|(_, o)| o).collect::<Vec<_>>();
        let validators = self
            .validators
            .into_iter()
            .map(|(_, v)| v)
            .collect::<Vec<_>>();
        let objects = create_genesis_objects(&mut genesis_ctx, &modules, &objects, &validators);

        let genesis = Genesis {
            objects,
            validator_set: validators,
        };

        // Verify that all the validators were properly created onchain
        let system_object = genesis.sui_system_object();
        assert_eq!(system_object.epoch, 0);

        for (validator, onchain_validator) in genesis
            .validator_set()
            .iter()
            .zip(system_object.validators.active_validators.iter())
        {
            assert_eq!(validator.stake(), onchain_validator.stake_amount);
            assert_eq!(
                validator.sui_address().to_vec(),
                onchain_validator.metadata.sui_address.to_vec(),
            );
            assert_eq!(
                validator.public_key().as_ref().to_vec(),
                onchain_validator.metadata.pubkey_bytes,
            );
            assert_eq!(validator.name().as_bytes(), onchain_validator.metadata.name);
            assert_eq!(
                validator.network_address().to_vec(),
                onchain_validator.metadata.net_address
            );
        }

        genesis
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, anyhow::Error> {
        let path = path.as_ref();
        let path: &Utf8Path = path.try_into()?;
        trace!("Reading Genesis Builder from {}", path);

        if !path.is_dir() {
            bail!("path must be a directory");
        }

        // Load Objects
        let mut objects = BTreeMap::new();
        for entry in path.join(GENESIS_BUILDER_OBJECT_DIR).read_dir_utf8()? {
            let entry = entry?;
            if entry.file_name().starts_with('.') {
                continue;
            }

            let path = entry.path();
            let object_bytes = fs::read(path)?;
            let object: Object = serde_yaml::from_slice(&object_bytes)?;
            objects.insert(object.id(), object);
        }

        // Load validator infos
        let mut committee = BTreeMap::new();
        for entry in path.join(GENESIS_BUILDER_COMMITTEE_DIR).read_dir_utf8()? {
            let entry = entry?;
            if entry.file_name().starts_with('.') {
                continue;
            }

            let path = entry.path();
            let validator_info_bytes = fs::read(path)?;
            let validator_info: ValidatorInfo = serde_yaml::from_slice(&validator_info_bytes)?;
            committee.insert(validator_info.public_key(), validator_info);
        }

        Ok(Self {
            objects,
            validators: committee,
        })
    }

    pub fn save<P: AsRef<Path>>(self, path: P) -> Result<(), anyhow::Error> {
        let path = path.as_ref();
        trace!("Writing Genesis Builder to {}", path.display());

        std::fs::create_dir_all(path)?;

        // Write Objects
        let object_dir = path.join(GENESIS_BUILDER_OBJECT_DIR);
        std::fs::create_dir_all(&object_dir)?;

        for (_id, object) in self.objects {
            let object_bytes = serde_yaml::to_vec(&object)?;
            let hex_digest = encode_bytes_hex(&object.digest());
            fs::write(object_dir.join(hex_digest), object_bytes)?;
        }

        // Write validator infos
        let committee_dir = path.join(GENESIS_BUILDER_COMMITTEE_DIR);
        std::fs::create_dir_all(&committee_dir)?;

        for (_pubkey, validator) in self.validators {
            let validator_info_bytes = serde_yaml::to_vec(&validator)?;
            let hex_name = encode_bytes_hex(&validator.public_key());
            fs::write(committee_dir.join(hex_name), validator_info_bytes)?;
        }

        Ok(())
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
        use std::collections::HashSet;
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
        TemporaryStore::new(&*store, InputObjects::new(filtered), ctx.digest());
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

    let (
        InnerTemporaryStore {
            written, deleted, ..
        },
        _events,
    ) = temporary_store.into_inner();

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
        TemporaryStore::new(&*store, InputObjects::new(vec![]), genesis_digest);

    let mut pubkeys = Vec::new();
    let mut network_pubkeys = Vec::new();
    let mut proof_of_possessions = Vec::new();
    let mut sui_addresses = Vec::new();
    let mut network_addresses = Vec::new();
    let mut names = Vec::new();
    let mut stakes = Vec::new();
    let mut gas_prices = Vec::new();

    for validator in committee {
        pubkeys.push(validator.public_key());
        network_pubkeys.push(validator.network_key());
        proof_of_possessions.push(validator.proof_of_possession().as_ref().to_vec());
        sui_addresses.push(validator.sui_address());
        network_addresses.push(validator.network_address());
        names.push(validator.name().to_owned().into_bytes());
        stakes.push(validator.stake());
        gas_prices.push(validator.gas_price());
    }

    adapter::execute(
        move_vm,
        &mut temporary_store,
        ModuleId::new(SUI_FRAMEWORK_ADDRESS, ident_str!("genesis").to_owned()),
        &ident_str!("create").to_owned(),
        vec![],
        vec![
            CallArg::Pure(bcs::to_bytes(&pubkeys).unwrap()),
            CallArg::Pure(bcs::to_bytes(&network_pubkeys).unwrap()),
            CallArg::Pure(bcs::to_bytes(&proof_of_possessions).unwrap()),
            CallArg::Pure(bcs::to_bytes(&sui_addresses).unwrap()),
            CallArg::Pure(bcs::to_bytes(&names).unwrap()),
            CallArg::Pure(bcs::to_bytes(&network_addresses).unwrap()),
            CallArg::Pure(bcs::to_bytes(&stakes).unwrap()),
            CallArg::Pure(bcs::to_bytes(&gas_prices).unwrap()),
        ],
        &mut SuiGasStatus::new_unmetered(),
        genesis_ctx,
    )?;

    let (
        InnerTemporaryStore {
            written, deleted, ..
        },
        _events,
    ) = temporary_store.into_inner();

    store.finish(written, deleted);

    Ok(())
}

const GENESIS_BUILDER_OBJECT_DIR: &str = "objects";
const GENESIS_BUILDER_COMMITTEE_DIR: &str = "committee";

#[cfg(test)]
mod test {
    use super::Builder;
    use crate::{genesis_config::GenesisConfig, utils, ValidatorInfo};
    use fastcrypto::traits::KeyPair;
    use sui_types::crypto::{
        generate_proof_of_possession, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
    };

    #[test]
    fn roundtrip() {
        let genesis = Builder::new().build();

        let s = serde_yaml::to_string(&genesis).unwrap();
        let from_s = serde_yaml::from_str(&s).unwrap();
        assert_eq!(genesis, from_s);
    }

    #[test]
    fn ceremony() {
        let dir = tempfile::TempDir::new().unwrap();

        let genesis_config = GenesisConfig::for_local_testing();
        let (_account_keys, objects) = genesis_config
            .generate_accounts(&mut rand::rngs::OsRng)
            .unwrap();

        let key: AuthorityKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let network_key: AccountKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let validator = ValidatorInfo {
            name: "0".into(),
            public_key: key.public().into(),
            network_key: network_key.public().clone().into(),
            stake: 1,
            delegation: 0,
            gas_price: 1,
            network_address: utils::new_network_address(),
            proof_of_possession: generate_proof_of_possession(&key),
            narwhal_primary_to_primary: utils::new_network_address(),
            narwhal_worker_to_primary: utils::new_network_address(),
            narwhal_primary_to_worker: utils::new_network_address(),
            narwhal_worker_to_worker: utils::new_network_address(),
            narwhal_consensus_address: utils::new_network_address(),
        };

        let builder = Builder::new().add_objects(objects).add_validator(validator);
        builder.save(dir.path()).unwrap();
        Builder::load(dir.path()).unwrap();
    }
}
