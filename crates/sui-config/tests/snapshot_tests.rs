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
use rand::rngs::StdRng;
use rand::SeedableRng;
use sui_config::genesis::{GenesisCeremonyParameters, TokenDistributionScheduleBuilder};
use sui_config::node::{DEFAULT_COMMISSION_RATE, DEFAULT_VALIDATOR_GAS_PRICE};
use sui_config::ValidatorInfo;
use sui_config::{genesis::Builder, genesis_config::GenesisConfig};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
    NetworkKeyPair, SuiKeyPair,
};
use sui_types::multiaddr::Multiaddr;

#[test]
#[cfg_attr(msim, ignore)]
fn genesis_config_snapshot_matches() {
    let ed_kp1: SuiKeyPair =
        SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut StdRng::from_seed([0; 32])).1);
    let fake_addr: SuiAddress = (&ed_kp1.public()).into();

    let mut genesis_config = GenesisConfig::for_local_testing();
    genesis_config.parameters.chain_start_timestamp_ms = 0;
    for account in &mut genesis_config.accounts {
        account.address = Some(fake_addr);
    }
    assert_yaml_snapshot!(genesis_config);
}

#[test]
fn populated_genesis_snapshot_matches() {
    let genesis_config = GenesisConfig::for_local_testing();
    let (_account_keys, allocations) = genesis_config
        .generate_accounts(&mut StdRng::from_seed([0; 32]))
        .unwrap();
    let mut rng = StdRng::from_seed([0; 32]);
    let key: AuthorityKeyPair = get_key_pair_from_rng(&mut rng).1;
    let worker_key: NetworkKeyPair = get_key_pair_from_rng(&mut rng).1;
    let network_key: NetworkKeyPair = get_key_pair_from_rng(&mut rng).1;
    let account_key: AccountKeyPair = get_key_pair_from_rng(&mut rng).1;
    let validator = ValidatorInfo {
        name: "0".into(),
        protocol_key: key.public().into(),
        worker_key: worker_key.public().clone(),
        account_address: SuiAddress::from(account_key.public()),
        network_key: network_key.public().clone(),
        gas_price: DEFAULT_VALIDATOR_GAS_PRICE,
        commission_rate: DEFAULT_COMMISSION_RATE,
        network_address: "/ip4/127.0.0.1/tcp/80".parse().unwrap(),
        p2p_address: "/ip4/127.0.0.1/udp/80".parse().unwrap(),
        narwhal_primary_address: "/ip4/127.0.0.1/udp/80".parse().unwrap(),
        narwhal_worker_address: "/ip4/127.0.0.1/udp/80".parse().unwrap(),
        description: String::new(),
        image_url: String::new(),
        project_url: String::new(),
    };
    let pop = generate_proof_of_possession(&key, account_key.public().into());

    let token_distribution_schedule = {
        let mut builder = TokenDistributionScheduleBuilder::new();
        for allocation in allocations {
            builder.add_allocation(allocation);
        }
        builder.default_allocation_for_validators(Some(validator.account_address));
        builder.build()
    };

    let genesis = Builder::new()
        .with_token_distribution_schedule(token_distribution_schedule)
        .add_validator(validator, pop)
        .with_parameters(GenesisCeremonyParameters {
            chain_start_timestamp_ms: 10,
            ..GenesisCeremonyParameters::new()
        })
        .add_validator_signature(&key)
        .build();
    assert_yaml_snapshot!(genesis.sui_system_wrapper_object());
    assert_yaml_snapshot!(genesis
        .sui_system_object()
        .into_genesis_version_for_tooling());
    assert_yaml_snapshot!(genesis.clock());
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
        validator_config.p2p_config.external_address = None;
        validator_config.admin_interface_port = 8888;
        let metrics_addr: Multiaddr = "/ip4/127.0.0.1/tcp/1234".parse().unwrap();
        let primary_network_admin_server_port = 5678;
        let worker_network_admin_server_base_port = 8765;
        if let Some(consensus_config) = validator_config.consensus_config.as_mut() {
            consensus_config.address = Multiaddr::empty();
            consensus_config.db_path = PathBuf::from("/tmp/foo/");
            consensus_config.internal_worker_address = Some(Multiaddr::empty());
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
