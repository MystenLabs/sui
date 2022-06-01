// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_binary_format::CompiledModule;
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use sui_types::base_types::{ObjectID, SuiAddress, TxContext};
use sui_types::committee::StakeUnit;
use sui_types::crypto::{get_key_pair_from_rng, KeyPair};
use sui_types::object::Object;
use tracing::info;

use crate::Config;

#[derive(Serialize, Deserialize)]
pub struct GenesisConfig {
    pub validator_genesis_info: Option<Vec<ValidatorGenesisInfo>>,
    pub committee_size: usize,
    pub accounts: Vec<AccountConfig>,
    pub move_packages: Vec<PathBuf>,
    pub sui_framework_lib_path: Option<PathBuf>,
    pub move_framework_lib_path: Option<PathBuf>,
}

impl Config for GenesisConfig {}

impl GenesisConfig {
    pub fn generate_accounts<R: ::rand::RngCore + ::rand::CryptoRng>(
        &self,
        mut rng: R,
    ) -> Result<(Vec<KeyPair>, Vec<Object>)> {
        let mut addresses = Vec::new();
        let mut preload_objects = Vec::new();
        let mut all_preload_objects_set = BTreeSet::new();

        info!("Creating accounts and gas objects...");

        let mut keys = Vec::new();
        for account in &self.accounts {
            let address = if let Some(address) = account.address {
                address
            } else {
                let (address, keypair) = get_key_pair_from_rng(&mut rng);
                keys.push(keypair);
                address
            };

            addresses.push(address);
            let mut preload_objects_map = BTreeMap::new();

            // Populate gas itemized objects
            account.gas_objects.iter().for_each(|q| {
                if !all_preload_objects_set.contains(&q.object_id) {
                    preload_objects_map.insert(q.object_id, q.gas_value);
                }
            });

            // Populate ranged gas objects
            if let Some(ranges) = &account.gas_object_ranges {
                for rg in ranges {
                    let ids = ObjectID::in_range(rg.offset, rg.count)?;

                    for obj_id in ids {
                        if !preload_objects_map.contains_key(&obj_id)
                            && !all_preload_objects_set.contains(&obj_id)
                        {
                            preload_objects_map.insert(obj_id, rg.gas_value);
                            all_preload_objects_set.insert(obj_id);
                        }
                    }
                }
            }

            for (object_id, value) in preload_objects_map {
                let new_object = Object::with_id_owner_gas_coin_object_for_testing(
                    object_id,
                    sui_types::base_types::SequenceNumber::new(),
                    address,
                    value,
                );
                preload_objects.push(new_object);
            }
        }

        Ok((keys, preload_objects))
    }

    pub fn generate_custom_move_modules(
        &self,
        genesis_ctx: &mut TxContext,
    ) -> Result<Vec<Vec<CompiledModule>>> {
        let mut custom_modules = Vec::new();
        // Build custom move packages
        if !self.move_packages.is_empty() {
            info!(
                "Loading {} Move packages from {:?}",
                self.move_packages.len(),
                self.move_packages,
            );

            for path in &self.move_packages {
                let mut modules = sui_framework::build_move_package(
                    path,
                    move_package::BuildConfig::default(),
                    false,
                )?;

                let package_id =
                    sui_adapter::adapter::generate_package_id(&mut modules, genesis_ctx)?;

                info!("Loaded package [{}] from {:?}.", package_id, path);
                custom_modules.push(modules)
            }
        }
        Ok(custom_modules)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ValidatorGenesisInfo {
    pub key_pair: KeyPair,
    pub network_address: Multiaddr,
    pub stake: StakeUnit,
    pub narwhal_primary_to_primary: Multiaddr,
    pub narwhal_worker_to_primary: Multiaddr,
    pub narwhal_primary_to_worker: Multiaddr,
    pub narwhal_worker_to_worker: Multiaddr,
    pub narwhal_consensus_address: Multiaddr,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AccountConfig {
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "SuiAddress::optional_address_as_hex",
        deserialize_with = "SuiAddress::optional_address_from_hex"
    )]
    pub address: Option<SuiAddress>,
    pub gas_objects: Vec<ObjectConfig>,
    pub gas_object_ranges: Option<Vec<ObjectConfigRange>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ObjectConfigRange {
    /// Starting object id
    pub offset: ObjectID,
    /// Number of object ids
    pub count: u64,
    /// Gas value per object id
    pub gas_value: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ObjectConfig {
    #[serde(default = "ObjectID::random")]
    pub object_id: ObjectID,
    #[serde(default = "default_gas_value")]
    pub gas_value: u64,
}

fn default_gas_value() -> u64 {
    DEFAULT_GAS_AMOUNT
}

const DEFAULT_GAS_AMOUNT: u64 = 100000;
const DEFAULT_NUMBER_OF_AUTHORITIES: usize = 4;
const DEFAULT_NUMBER_OF_ACCOUNT: usize = 5;
const DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT: usize = 5;

impl GenesisConfig {
    pub fn for_local_testing() -> Result<Self, anyhow::Error> {
        Self::custom_genesis(
            DEFAULT_NUMBER_OF_AUTHORITIES,
            DEFAULT_NUMBER_OF_ACCOUNT,
            DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT,
        )
    }

    pub fn custom_genesis(
        num_authorities: usize,
        num_accounts: usize,
        num_objects_per_account: usize,
    ) -> Result<Self, anyhow::Error> {
        assert!(
            num_authorities > 0,
            "num_authorities should be larger than 0"
        );

        let mut accounts = Vec::new();
        for _ in 0..num_accounts {
            let mut objects = Vec::new();
            for _ in 0..num_objects_per_account {
                objects.push(ObjectConfig {
                    object_id: ObjectID::random(),
                    gas_value: DEFAULT_GAS_AMOUNT,
                })
            }
            accounts.push(AccountConfig {
                address: None,
                gas_objects: objects,
                gas_object_ranges: Some(Vec::new()),
            })
        }

        Ok(Self {
            accounts,
            ..Default::default()
        })
    }
}

impl Default for GenesisConfig {
    fn default() -> Self {
        Self {
            validator_genesis_info: None,
            committee_size: DEFAULT_NUMBER_OF_AUTHORITIES,
            accounts: vec![],
            move_packages: vec![],
            sui_framework_lib_path: None,
            move_framework_lib_path: None,
        }
    }
}
