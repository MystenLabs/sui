// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use insta::assert_yaml_snapshot;
use multiaddr::Multiaddr;
use narwhal_crypto::traits::KeyPair;
use rand::rngs::StdRng;
use rand::SeedableRng;
use sui_config::{genesis::Builder, genesis_config::GenesisConfig};
use sui_config::{NetworkConfig, ValidatorInfo};
use sui_types::crypto::get_key_pair_from_rng;

#[test]
fn genesis_config_snapshot_matches() {
    let genesis_config = GenesisConfig::for_local_testing();
    assert_yaml_snapshot!(genesis_config, {
        ".accounts[].gas_objects[].object_id" => "[fake object id]"
    });
}

#[test]
fn empty_genesis_snapshot_matches() {
    let genesis = Builder::new().build();
    assert_yaml_snapshot!(genesis);
}

#[test]
fn populated_genesis_snapshot_matches() {
    let genesis_config = GenesisConfig::for_local_testing();
    let (_account_keys, objects) = genesis_config
        .generate_accounts(&mut StdRng::from_seed([0; 32]))
        .unwrap();
    let key = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
    let validator = ValidatorInfo {
        name: "0".into(),
        public_key: key.public().into(),
        stake: 1,
        delegation: 0,
        network_address: Multiaddr::empty(),
        narwhal_primary_to_primary: Multiaddr::empty(),
        narwhal_worker_to_primary: Multiaddr::empty(),
        narwhal_primary_to_worker: Multiaddr::empty(),
        narwhal_worker_to_worker: Multiaddr::empty(),
        narwhal_consensus_address: Multiaddr::empty(),
    };

    let genesis = Builder::new()
        .add_objects(objects)
        .add_validator(validator)
        .build();
    assert_yaml_snapshot!(genesis.validator_set());
    assert_yaml_snapshot!(genesis.committee().unwrap());
    assert_yaml_snapshot!(genesis.narwhal_committee());
    assert_yaml_snapshot!(genesis.sui_system_object());
}

#[test]
fn network_config_snapshot_matches() {
    let temp_dir = tempfile::tempdir().unwrap();
    let committee_size = 7;
    let rng = StdRng::from_seed([0; 32]);
    let mut network_config = NetworkConfig::generate_with_rng(temp_dir.path(), committee_size, rng);
    // TODO: Inject static temp path and port numbers, instead of clearing them.
    for mut validator_config in &mut network_config.validator_configs {
        validator_config.db_path = PathBuf::from("/tmp/foo/");
        validator_config.network_address = Multiaddr::empty();
        let fake_socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 1);
        validator_config.json_rpc_address = fake_socket;
        validator_config.metrics_address = fake_socket;
        validator_config.admin_interface_port = 8888;
        if let Some(consensus_config) = validator_config.consensus_config.as_mut() {
            consensus_config.consensus_address = Multiaddr::empty();
            consensus_config.consensus_db_path = PathBuf::from("/tmp/foo/");
            consensus_config
                .narwhal_config
                .consensus_api_grpc
                .socket_addr = Multiaddr::empty();
            consensus_config
                .narwhal_config
                .prometheus_metrics
                .socket_addr = fake_socket;
        }
    }
    assert_yaml_snapshot!(network_config, {
        ".genesis" => "[fake genesis]",
        ".validator_configs[].genesis.genesis" => "[fake genesis]",
    });
}
