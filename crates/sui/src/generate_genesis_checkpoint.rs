// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use camino::Utf8PathBuf;
use sui_config::genesis::Builder;
use sui_config::utils;
use sui_config::ValidatorInfo;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
    KeypairTraits, NetworkKeyPair,
};

#[tokio::main]
async fn main() {
    let dir = std::env::current_dir().unwrap();
    let dir = Utf8PathBuf::try_from(dir).unwrap();

    let mut builder = Builder::new();
    let mut keys = Vec::new();
    for i in 0..2 {
        let key: AuthorityKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let worker_key: NetworkKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let account_key: AccountKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let network_key: NetworkKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
        let validator = ValidatorInfo {
            name: format!("Validator {}", i),
            protocol_key: key.public().into(),
            worker_key: worker_key.public().clone(),
            account_address: SuiAddress::from(account_key.public()),
            network_key: network_key.public().clone(),
            gas_price: sui_config::node::DEFAULT_VALIDATOR_GAS_PRICE,
            commission_rate: sui_config::node::DEFAULT_COMMISSION_RATE,
            network_address: utils::new_tcp_network_address(),
            p2p_address: utils::new_udp_network_address(),
            narwhal_primary_address: utils::new_udp_network_address(),
            narwhal_worker_address: utils::new_udp_network_address(),
            description: String::new(),
            image_url: String::new(),
            project_url: String::new(),
        };
        let pop = generate_proof_of_possession(&key, account_key.public().into());
        keys.push(key);
        builder = builder.add_validator(validator, pop);
    }

    for key in keys {
        builder = builder.add_validator_signature(&key);
    }

    builder.save(dir).unwrap();
}
