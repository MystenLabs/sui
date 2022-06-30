// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use std::path::Path;
use sui_config::genesis_config::{AccountConfig, GenesisConfig, ObjectConfigRange};
use sui_config::Config;
use sui_types::crypto::KeyPair;

use sui_types::{base_types::ObjectID};

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

    // For each load generator, create an address as the source of transfers, and configs
    let mut accounts = vec![];
    let mut obj_id_offset = bch.object_id_offset;
    let mut account_private_info = vec![];

    // For each load gen, create an account
    for _ in 0..bch.number_of_generators {
        // Create a keypair for this account
        let (account_address, account_keypair) = KeyPair::get_key_pair();

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
    let genesis_config = GenesisConfig {
        validator_genesis_info: None,
        committee_size: bch.host_port_stake_triplets.len(),
        accounts: accounts.clone(),
        move_packages: vec![],
        sui_framework_lib_path: None,
        move_framework_lib_path: None,
    };

    let path_str = "distributed_bench_genesis.conf";
    let genesis_path = Path::new(&path_str);
    genesis_config.persisted(genesis_path).save().unwrap();
}

fn _parse_host_port_stake_triplet(s: &str) -> (String, u16, usize) {
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
