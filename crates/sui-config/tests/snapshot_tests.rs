// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This file contains tests that detect changes in Sui configs.
// If a PR breaks one or more tests here, the PR probably has a real impact
// on a production configuration file. When test failure happens, the PR should
// be marked as a breaking change and reviewers should be aware of this.
//
// Owners and operators of production configuration files can add themselves to
// .github/CODEOWNERS for the corresponding snapshot tests, so they can get notified
// of changes. PRs that modifies snapshot files should wait for reviews from
// code owners (if any) before merging.
//
// To review snapshot changes, and fix snapshot differences,
// 0. Install cargo-insta
// 1. Run `cargo insta test --review` under `./sui-config`.
// 2. Review, accept or reject changes.

use fastcrypto::traits::KeyPair;
use insta::assert_yaml_snapshot;
use multiaddr::Multiaddr;
use rand::rngs::StdRng;
use rand::SeedableRng;
use sui_config::ValidatorInfo;
use sui_config::{genesis::Builder, genesis_config::GenesisConfig};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
    AuthorityPublicKeyBytes, NetworkKeyPair,
};

#[test]
fn genesis_config_snapshot_matches() {
    // Test creating fake SuiAddress from PublicKeyBytes.
    let keypair: AuthorityKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
    let public_key = AuthorityPublicKeyBytes::from(keypair.public());
    let fake_addr = SuiAddress::from(&public_key);

    let fake_obj_id = ObjectID::from(fake_addr);
    let mut genesis_config = GenesisConfig::for_local_testing();
    for account in &mut genesis_config.accounts {
        account.address = Some(fake_addr);
        for gas_obj in &mut account.gas_objects {
            gas_obj.object_id = fake_obj_id;
        }
    }
    assert_yaml_snapshot!(genesis_config);
}

// If a Move change breaks the test below, it is because related Move functions are included
// in the genesis blob. The test failure can be fixed by updating the snapshot / blob.
// Also, folks in the file's CODEWOWNERS should be notified about the change. They may need to
// re-create the genesis blob.
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
    let key: AuthorityKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
    let worker_key: NetworkKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
    let network_key: NetworkKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
    let account_key: AccountKeyPair = get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1;
    let validator = ValidatorInfo {
        name: "0".into(),
        protocol_key: key.public().into(),
        worker_key: worker_key.public().clone(),
        account_key: account_key.public().clone().into(),
        network_key: network_key.public().clone(),
        stake: 1,
        delegation: 0,
        gas_price: 1,
        commission_rate: 0,
        network_address: Multiaddr::empty(),
        narwhal_primary_address: Multiaddr::empty(),
        narwhal_worker_address: Multiaddr::empty(),
        narwhal_internal_worker_address: None,
        narwhal_consensus_address: Multiaddr::empty(),
    };
    let pop = generate_proof_of_possession(&key, account_key.public().into());

    let genesis = Builder::new()
        .add_objects(objects)
        .add_validator(validator, pop)
        .build();
    assert_yaml_snapshot!(genesis.validator_set());
    assert_yaml_snapshot!(genesis.committee().unwrap());
    assert_yaml_snapshot!(genesis.narwhal_committee());
    assert_yaml_snapshot!(genesis.narwhal_worker_cache());
    assert_yaml_snapshot!(genesis.sui_system_object());
    // Serialized `genesis` is not static and cannot be snapshot tested.
}

#[test]
#[cfg_attr(msim, ignore)]
fn network_config_snapshot_matches() {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::path::PathBuf;
    use sui_config::NetworkConfig;

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
        validator_config.p2p_config.listen_address = fake_socket;
        validator_config.admin_interface_port = 8888;
        let metrics_addr: Multiaddr = "/ip4/127.0.0.1/tcp/1234".parse().unwrap();
        let primary_network_admin_server_port = 5678;
        let worker_network_admin_server_base_port = 8765;
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
                .socket_addr = metrics_addr;
            consensus_config
                .narwhal_config
                .network_admin_server
                .primary_network_admin_server_port = primary_network_admin_server_port;
            consensus_config
                .narwhal_config
                .network_admin_server
                .worker_network_admin_server_base_port = worker_network_admin_server_base_port;
        }
    }
    assert_yaml_snapshot!(network_config, {
        ".genesis" => "[fake genesis]",
        ".validator_configs[].genesis.genesis" => "[fake genesis]",
    });
}
