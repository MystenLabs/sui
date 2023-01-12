// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::LightClient;
use prometheus::Registry;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use sui_config::node::AuthorityStorePruningConfig;
use sui_config::{utils, NetworkConfig};
use sui_core::authority::AuthorityStore;
use sui_core::checkpoints::CheckpointStore;
use sui_core::epoch::committee_store::CommitteeStore;
use sui_core::storage::RocksDbStore;
use sui_macros::{nondeterministic, sim_test};
use sui_types::base_types::ObjectID;
use sui_types::crypto::get_key_pair;
use test_utils::authority::{
    spawn_test_authorities, test_authority_configs, trigger_reconfiguration,
};

#[sim_test]
async fn test_light_client_get_committee() {
    let configs = test_authority_configs();
    let authorities = spawn_test_authorities([].into_iter(), &configs).await;
    trigger_reconfiguration(&authorities).await;

    // Give the light client enough time to sync to the latest epoch.
    let light_client = create_local_light_client(&configs).await;
    tokio::time::sleep(Duration::from_secs(20)).await;
    assert_eq!(
        light_client.get_committee(1).unwrap().unwrap(),
        authorities[0].with(|node| node.state().clone_committee_for_testing())
    );
}

async fn create_local_light_client(configs: &NetworkConfig) -> LightClient<RocksDbStore> {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("DB_{:?}", nondeterministic!(ObjectID::random())));
    std::fs::create_dir(&path).unwrap();
    let committee_store = Arc::new(CommitteeStore::new(
        path.join("committee"),
        &configs.committee(),
        None,
    ));
    let authority_store = Arc::new(
        AuthorityStore::open(
            path.join("authority").as_path(),
            None,
            &configs.genesis,
            &committee_store,
            &AuthorityStorePruningConfig::validator_config(),
        )
        .await
        .unwrap(),
    );
    let store = RocksDbStore::new(
        authority_store,
        committee_store.clone(),
        CheckpointStore::new(path.join("checkpoints").as_path()),
    );
    let seed_peers = configs
        .genesis
        .validator_set()
        .iter()
        .map(|validator| sui_config::p2p::SeedPeer {
            peer_id: Some(anemo::PeerId(validator.network_key.0.to_bytes())),
            address: validator.p2p_address.clone(),
        })
        .collect();
    let listen_ip = utils::get_local_ip_for_tests();
    let listen_ip_str = format!("{}", listen_ip);
    let address = SocketAddr::new(listen_ip, utils::get_available_port(&listen_ip_str));

    LightClient::new(
        store,
        address,
        seed_peers,
        get_key_pair().1,
        &Registry::new(),
    )
    .unwrap()
}
