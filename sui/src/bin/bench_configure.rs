// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(warnings)]

use clap::*;
use std::collections::BTreeMap;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
};
use sui::{
    benchmark::bench_types::RemoteLoadGenConfig,
    config::{
        AccountConfig, AuthorityInfo, Config, GenesisConfig, NetworkConfig, ObjectConfigRange,
    },
};

use sui_types::{base_types::ObjectID, crypto::get_key_pair};

const BUFFER_SIZE: usize = 650000;

#[derive(Debug, Parser)]
#[clap(
    name = "Sui Distributed Benchmark Config Creator",
    about = "Creates the config files for distributed benchmarking"
)]
pub struct DistributedBenchmarkConfigurator {
    /// List of space separated strings of the form: host:port:stake, example 127.0.0.1:8080:5
    #[clap(long, multiple_values = true)]
    pub host_port_stake_triplets: Vec<String>,
    #[clap(long, default_value = "0x0000000000000000000000000010000000000000")]
    pub object_id_offset: ObjectID,
    #[clap(long, default_value = "4")]
    pub number_of_generators: usize,
    #[clap(long, default_value = "1000000")]
    pub number_of_txes_per_generator: usize,
}
fn main() {
    let bch = DistributedBenchmarkConfigurator::parse();

    let mut authorities = vec![];
    let mut authority_keys = BTreeMap::new();

    // Create configs for authorities
    for b in bch.host_port_stake_triplets {
        let (host, port, stake) = parse_host_port_stake_triplet(&b);
        let (validator_address, validator_keypair) = get_key_pair();
        let db_path = format!("DB_{}", validator_address);
        let path = Path::new(&db_path);

        let host_bytes: Vec<u8> = host
            .split('.')
            .into_iter()
            .map(|q| q.parse::<u8>().unwrap())
            .collect();
        let consensus_address = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(
                host_bytes[0],
                host_bytes[1],
                host_bytes[2],
                host_bytes[3],
            )),
            port + 1,
        );

        let auth = AuthorityInfo {
            address: validator_address,
            host,
            port,
            db_path: path.to_path_buf(),
            stake,
            consensus_address,
            public_key: *validator_keypair.public_key_bytes(),
        };

        authorities.push(auth);
        authority_keys.insert(*validator_keypair.public_key_bytes(), validator_keypair);
    }

    // For each load generator, create an address as the source of transfers, and configs
    let mut accounts = vec![];
    let mut obj_id_offset = bch.object_id_offset;
    let mut account_private_info = vec![];

    // For each load gen, create an account
    for _ in 0..bch.number_of_generators {
        // Create a keypair for this account
        let (account_address, account_keypair) = get_key_pair();

        // Populate the range configs
        let range_cfg = ObjectConfigRange {
            offset: obj_id_offset,
            count: bch.number_of_txes_per_generator as u64,
            gas_value: u64::MAX,
        };

        account_private_info.push((account_keypair, obj_id_offset));

        // Ensure no overlap
        obj_id_offset = obj_id_offset
            .advance(bch.number_of_txes_per_generator)
            .unwrap();
        let account = AccountConfig {
            address: Some(account_address),
            gas_objects: vec![],
            gas_object_ranges: Some(vec![range_cfg]),
        };
        accounts.push(account);
    }

    // For each validator, fill in the account info into genesis
    for (i, (_, (_, kp))) in authorities.iter().zip(authority_keys.iter()).enumerate() {
        // Create and save the genesis configs for the validators
        let genesis_config = GenesisConfig {
            authorities: authorities.clone(),
            accounts: accounts.clone(),
            move_packages: vec![],
            sui_framework_lib_path: Path::new("../../sui_programmability/framework").to_path_buf(),
            move_framework_lib_path: Path::new(
                "../../sui_programmability/framework/deps/move-stdlib",
            )
            .to_path_buf(),
            key_pair: kp.copy(),
        };
        let path_str = format!("distributed_bench_genesis_{}.conf", i);
        let genesis_path = Path::new(&path_str);
        genesis_config.persisted(genesis_path).save().unwrap();
    }

    // For each load gen, provide configs and kps
    for (i, (_account_cfg, (account_keypair, object_id_offset))) in accounts
        .iter()
        .zip(account_private_info)
        .into_iter()
        .enumerate()
    {
        // Create and save network configs
        let network_config = NetworkConfig {
            epoch: 0,
            authorities: authorities.clone(),
            buffer_size: BUFFER_SIZE,
            loaded_move_packages: vec![],
            // This keypair is not important in this field
            key_pair: authority_keys.iter().last().unwrap().1.copy(),
        };

        // Create and save load gen configs
        let lg = RemoteLoadGenConfig {
            account_keypair,
            object_id_offset,
            validator_keypairs: authority_keys.iter().map(|q| (*q.0, q.1.copy())).collect(),
            network_config,
        };
        let path_str = format!("load_gen_{}.conf", i);
        let load_gen_path = Path::new(&path_str);

        lg.persisted(load_gen_path).save().unwrap();
    }
}

fn parse_host_port_stake_triplet(s: &str) -> (String, u16, usize) {
    let tokens: Vec<String> = s.split(':').into_iter().map(|t| t.to_owned()).collect();
    assert_eq!(tokens.len(), 3);

    let host = tokens[0].clone();

    #[allow(clippy::needless_collect)]
    let host_bytes = host
        .split('.')
        .into_iter()
        .map(|q| q.parse::<u8>().unwrap())
        .collect::<Vec<u8>>();
    assert_eq!(host_bytes.len(), 4);

    (
        host,
        tokens[1].parse::<u16>().unwrap(),
        tokens[2].parse::<usize>().unwrap(),
    )
}
