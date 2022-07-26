// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use insta::assert_yaml_snapshot;
use multiaddr::Multiaddr;
use rand::rngs::StdRng;
use rand::SeedableRng;
use sui_config::NetworkConfig;
use sui_config::{genesis::Builder, genesis_config::GenesisConfig};

#[test]
fn genesis_snapshot_matches() {
    let genesis = Builder::new().build();
    assert_yaml_snapshot!(genesis);
}

#[test]
fn genesis_config_snapshot_matches() {
    let genesis_config = GenesisConfig::for_local_testing();
    assert_yaml_snapshot!(genesis_config, {
        ".accounts[].gas_objects[].object_id" => "[fake object id]"
    });
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
