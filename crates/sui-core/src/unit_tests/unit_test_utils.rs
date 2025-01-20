// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::{test_authority_builder::TestAuthorityBuilder, AuthorityState};
use crate::authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder, TimeoutConfig};
use crate::test_authority_clients::LocalAuthorityClient;
use fastcrypto::traits::KeyPair;
use futures::future::join_all;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use sui_config::genesis::Genesis;
use sui_config::local_ip_utils;
use sui_config::node::AuthorityOverloadConfig;
use sui_framework::BuiltInFramework;
use sui_genesis_builder::validator_info::ValidatorInfo;
use sui_move_build::test_utils::compile_basics_package;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::crypto::AuthorityKeyPair;
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair, AccountKeyPair, AuthorityPublicKeyBytes,
    NetworkKeyPair, SuiKeyPair,
};
use sui_types::object::Object;

async fn init_genesis(
    committee_size: usize,
    mut genesis_objects: Vec<Object>,
) -> (
    Genesis,
    Vec<(AuthorityPublicKeyBytes, AuthorityKeyPair)>,
    ObjectID,
) {
    // add object_basics package object to genesis
    let modules: Vec<_> = compile_basics_package().get_modules().cloned().collect();
    let genesis_move_packages: Vec<_> = BuiltInFramework::genesis_move_packages().collect();
    let config = ProtocolConfig::get_for_max_version_UNSAFE();
    let pkg = Object::new_package(
        &modules,
        TransactionDigest::genesis_marker(),
        config.max_move_package_size(),
        config.move_binary_format_version(),
        &genesis_move_packages,
    )
    .unwrap();
    let pkg_id = pkg.id();
    genesis_objects.push(pkg);

    let mut builder = sui_genesis_builder::Builder::new().add_objects(genesis_objects);
    let mut key_pairs = Vec::new();
    for i in 0..committee_size {
        let key_pair: AuthorityKeyPair = get_key_pair().1;
        let authority_name = key_pair.public().into();
        let worker_key_pair: NetworkKeyPair = get_key_pair().1;
        let worker_name = worker_key_pair.public().clone();
        let account_key_pair: SuiKeyPair = get_key_pair::<AccountKeyPair>().1.into();
        let network_key_pair: NetworkKeyPair = get_key_pair().1;
        let validator_info = ValidatorInfo {
            name: format!("validator-{i}"),
            protocol_key: authority_name,
            worker_key: worker_name,
            account_address: SuiAddress::from(&account_key_pair.public()),
            network_key: network_key_pair.public().clone(),
            gas_price: 1,
            commission_rate: 0,
            network_address: local_ip_utils::new_local_tcp_address_for_testing(),
            p2p_address: local_ip_utils::new_local_udp_address_for_testing(),
            narwhal_primary_address: local_ip_utils::new_local_udp_address_for_testing(),
            narwhal_worker_address: local_ip_utils::new_local_udp_address_for_testing(),
            description: String::new(),
            image_url: String::new(),
            project_url: String::new(),
        };
        let pop = generate_proof_of_possession(&key_pair, (&account_key_pair.public()).into());
        builder = builder.add_validator(validator_info, pop);
        key_pairs.push((authority_name, key_pair));
    }
    for (_, key) in &key_pairs {
        builder = builder.add_validator_signature(key);
    }
    let genesis = builder.build();
    (genesis, key_pairs, pkg_id)
}

#[cfg(test)]
pub async fn init_local_authorities(
    committee_size: usize,
    genesis_objects: Vec<Object>,
) -> (
    AuthorityAggregator<LocalAuthorityClient>,
    Vec<Arc<AuthorityState>>,
    Genesis,
    ObjectID,
) {
    let (genesis, key_pairs, framework) = init_genesis(committee_size, genesis_objects).await;
    let authorities = join_all(key_pairs.iter().map(|(_, key_pair)| {
        TestAuthorityBuilder::new()
            .with_genesis_and_keypair(&genesis, key_pair)
            .build()
    }))
    .await;
    let aggregator = init_local_authorities_with_genesis(&genesis, authorities.clone()).await;
    (aggregator, authorities, genesis, framework)
}

#[cfg(test)]
pub async fn init_local_authorities_with_overload_thresholds(
    committee_size: usize,
    genesis_objects: Vec<Object>,
    overload_thresholds: AuthorityOverloadConfig,
) -> (
    AuthorityAggregator<LocalAuthorityClient>,
    Vec<Arc<AuthorityState>>,
    Genesis,
    ObjectID,
) {
    let (genesis, key_pairs, framework) = init_genesis(committee_size, genesis_objects).await;
    let authorities = join_all(key_pairs.iter().map(|(_, key_pair)| {
        TestAuthorityBuilder::new()
            .with_genesis_and_keypair(&genesis, key_pair)
            .with_authority_overload_config(overload_thresholds.clone())
            .build()
    }))
    .await;
    let aggregator = init_local_authorities_with_genesis(&genesis, authorities.clone()).await;
    (aggregator, authorities, genesis, framework)
}

#[cfg(test)]
pub async fn init_local_authorities_with_genesis(
    genesis: &Genesis,
    authorities: Vec<Arc<AuthorityState>>,
) -> AuthorityAggregator<LocalAuthorityClient> {
    telemetry_subscribers::init_for_testing();
    let mut clients = BTreeMap::new();
    for state in authorities {
        let name = state.name;
        let client = LocalAuthorityClient::new_from_authority(state);
        clients.insert(name, client);
    }
    let timeouts = TimeoutConfig {
        pre_quorum_timeout: Duration::from_secs(5),
        post_quorum_timeout: Duration::from_secs(5),
        serial_authority_request_interval: Duration::from_secs(1),
    };
    AuthorityAggregatorBuilder::from_genesis(genesis)
        .with_timeouts_config(timeouts)
        .build_custom_clients(clients)
}
