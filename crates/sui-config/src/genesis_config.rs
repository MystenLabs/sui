// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::{BTreeMap, BTreeSet};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::committee::StakeUnit;
use sui_types::crypto::{
    get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, AuthoritySignature, SuiKeyPair,
};
use sui_types::object::Object;
use sui_types::sui_serde::KeyPairBase64;
use tracing::info;

use crate::node::DEFAULT_GRPC_CONCURRENCY_LIMIT;
use crate::Config;

#[derive(Serialize, Deserialize)]
pub struct GenesisConfig {
    pub validator_genesis_info: Option<Vec<ValidatorGenesisInfo>>,
    pub committee_size: usize,
    pub grpc_load_shed: Option<bool>,
    pub grpc_concurrency_limit: Option<usize>,
    pub accounts: Vec<AccountConfig>,
}

impl Config for GenesisConfig {}

impl GenesisConfig {
    pub fn generate_accounts<R: ::rand::RngCore + ::rand::CryptoRng>(
        &self,
        mut rng: R,
    ) -> Result<(Vec<AccountKeyPair>, Vec<Object>)> {
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
                let new_object = Object::with_id_owner_gas_for_testing(object_id, address, value);
                preload_objects.push(new_object);
            }
        }

        Ok((keys, preload_objects))
    }
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct ValidatorGenesisInfo {
    #[serde_as(as = "KeyPairBase64")]
    pub key_pair: AuthorityKeyPair,
    pub account_key_pair: SuiKeyPair,
    pub network_key_pair: SuiKeyPair,
    pub proof_of_possession: AuthoritySignature,
    pub network_address: Multiaddr,
    pub stake: StakeUnit,
    pub gas_price: u64,
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

const DEFAULT_GAS_AMOUNT: u64 = 100000000;
const DEFAULT_NUMBER_OF_AUTHORITIES: usize = 4;
const DEFAULT_NUMBER_OF_ACCOUNT: usize = 5;
const DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT: usize = 5;

impl GenesisConfig {
    pub fn for_local_testing() -> Self {
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
    ) -> Self {
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

        Self {
            accounts,
            ..Default::default()
        }
    }
}

impl Default for GenesisConfig {
    fn default() -> Self {
        Self {
            validator_genesis_info: None,
            committee_size: DEFAULT_NUMBER_OF_AUTHORITIES,
            grpc_load_shed: None,
            grpc_concurrency_limit: Some(DEFAULT_GRPC_CONCURRENCY_LIMIT),
            accounts: vec![],
        }
    }
}
