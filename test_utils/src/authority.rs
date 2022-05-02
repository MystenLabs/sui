// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::test_committee;
use crate::test_keys;
use narwhal_config::Parameters as ConsensusParameters;
use std::path::PathBuf;
use std::sync::Arc;
use sui::config::{make_default_narwhal_committee, AuthorityPrivateInfo, PORT_ALLOCATOR};
use sui::sui_commands::make_authority;
use sui_adapter::genesis;
use sui_core::authority::AuthorityState;
use sui_core::authority::AuthorityStore;
use sui_core::authority_server::AuthorityServer;
use sui_network::transport::SpawnedServer;
use sui_types::object::Object;

/// The default network buffer size of a test authority.
pub const NETWORK_BUFFER_SIZE: usize = 65_000;

/// Make a test authority store in a temporary directory.
pub fn test_authority_store() -> AuthorityStore {
    let store_path = tempfile::tempdir().unwrap();
    AuthorityStore::open(store_path, None)
}

/// Make an authority config for each of the `TEST_COMMITTEE_SIZE` authorities in the test committee.
pub fn test_authority_configs() -> Vec<AuthorityPrivateInfo> {
    test_keys()
        .into_iter()
        .map(|(address, key)| {
            let authority_port = PORT_ALLOCATOR.lock().unwrap().next_port().unwrap();
            let consensus_port = PORT_ALLOCATOR.lock().unwrap().next_port().unwrap();

            AuthorityPrivateInfo {
                address,
                key_pair: key,
                host: "127.0.0.1".to_string(),
                port: authority_port,
                db_path: PathBuf::new(),
                stake: 1,
                consensus_address: format!("127.0.0.1:{consensus_port}").parse().unwrap(),
            }
        })
        .collect()
}

/// Make a test authority state for each committee member.
pub async fn test_authority_states<I>(objects: I) -> Vec<AuthorityState>
where
    I: IntoIterator<Item = Object> + Clone,
{
    let committee = test_committee();
    let mut authorities = Vec::new();
    for (_, key) in test_keys() {
        let state = AuthorityState::new(
            committee.clone(),
            *key.public_key_bytes(),
            Arc::pin(key),
            Arc::new(test_authority_store()),
            genesis::clone_genesis_compiled_modules(),
            &mut genesis::get_genesis_context(),
        )
        .await;

        for o in objects.clone() {
            state.insert_genesis_object(o).await;
        }

        authorities.push(state);
    }
    authorities
}

/// Spawn all authorities in the test committee into a separate tokio task.
pub async fn spawn_test_authorities<I>(
    objects: I,
    configs: &[AuthorityPrivateInfo],
) -> Vec<SpawnedServer<AuthorityServer>>
where
    I: IntoIterator<Item = Object> + Clone,
{
    let states = test_authority_states(objects).await;
    let consensus_committee = make_default_narwhal_committee(configs).unwrap();
    let consensus_parameters = ConsensusParameters {
        max_header_delay: std::time::Duration::from_millis(200),
        header_size: 1,
        ..ConsensusParameters::default()
    };
    let mut handles = Vec::new();
    for (state, config) in states.into_iter().zip(configs.iter()) {
        let handle = make_authority(
            /* authority */ config,
            NETWORK_BUFFER_SIZE,
            state,
            &consensus_committee,
            /* consensus_store_path */ tempfile::tempdir().unwrap().path(),
            &consensus_parameters,
            /* net_parameters */ None,
        )
        .await
        .unwrap()
        .spawn()
        .await
        .unwrap();
        handles.push(handle);
    }
    handles
}
