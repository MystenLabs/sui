#![deny(warnings)]

use clap::*;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::Path,
};
use sui::{
    benchmark::bench_types::RemoteLoadGenConfig,
    config::{
        AccountConfig, AuthorityPrivateInfo, Config, GenesisConfig, NetworkConfig,
        ObjectConfigRange,
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
    let mut authorities_copy = vec![];

    for b in bch.host_port_stake_triplets {
        let (host, port, stake) = parse_host_port_stake_triplet(&b);
        let (addr, kp) = get_key_pair();
        let db_path = format!("DB_{}", addr);
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

        let auth = AuthorityPrivateInfo {
            address: addr,
            key_pair: kp,
            host,
            port,
            db_path: path.to_path_buf(),
            stake,
            consensus_address,
        };

        authorities.push(auth.copy());
        authorities_copy.push(auth);
    }

    // For each load generator, create an address as the source of transfers
    let mut accounts = vec![];
    let mut obj_id_offset = bch.object_id_offset;
    let mut account_private_info = vec![];
    for _ in 0..bch.number_of_generators {
        let (address, kp) = get_key_pair();

        let range_cfg = ObjectConfigRange {
            offset: obj_id_offset,
            count: bch.number_of_txes_per_generator as u64,
            gas_value: u64::MAX,
        };

        account_private_info.push((kp, obj_id_offset));

        // Ensure no overlap
        obj_id_offset = obj_id_offset
            .advance(bch.number_of_txes_per_generator)
            .unwrap();

        let account = AccountConfig {
            address: Some(address),
            gas_objects: vec![],
            gas_object_ranges: Some(vec![range_cfg]),
        };
        accounts.push(account);
    }

    // Create and save the genesis configs for the validators
    let genesis_config = GenesisConfig {
        authorities,
        accounts,
        move_packages: vec![],
        sui_framework_lib_path: Path::new("../../sui_programmability/framework").to_path_buf(),
        move_framework_lib_path: Path::new("../../sui_programmability/framework/deps/move-stdlib")
            .to_path_buf(),
    };
    let genesis_path = Path::new("distributed_bench_genesis.conf");
    genesis_config.persisted(genesis_path).save().unwrap();

    let network_config = NetworkConfig {
        epoch: 0,
        authorities: authorities_copy,
        buffer_size: BUFFER_SIZE,
        loaded_move_packages: vec![],
    };
    let network_path = Path::new("distributed_bench_network.conf");
    network_config.persisted(network_path).save().unwrap();

    // Generate the configs for each load generator
    for (i, (kp, offset)) in account_private_info.into_iter().enumerate() {
        let lg = RemoteLoadGenConfig {
            account_keypair: kp,
            object_id_offset: offset,
            network_cfg_path: network_path.to_path_buf(),
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
