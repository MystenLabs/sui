// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::TEST_COMMITTEE_SIZE;
use rand::{prelude::StdRng, SeedableRng};
use std::time::Duration;
use std::{collections::BTreeMap, sync::Arc};
use sui::sui_commands::make_authority;
use sui_config::{NetworkConfig, ValidatorInfo};
use sui_core::{
    authority::{AuthorityState, AuthorityStore},
    authority_aggregator::AuthorityAggregator,
    authority_client::{AuthorityClient, NetworkAuthorityClient},
    authority_server::AuthorityServerHandle,
    safe_client::SafeClient,
};
use sui_types::{committee::Committee, object::Object};

/// The default network buffer size of a test authority.
pub const NETWORK_BUFFER_SIZE: usize = 65_000;

/// Make a test authority store in a temporary directory.
pub fn test_authority_store() -> AuthorityStore {
    let store_path = tempfile::tempdir().unwrap();
    AuthorityStore::open(store_path, None)
}

/// Make an authority config for each of the `TEST_COMMITTEE_SIZE` authorities in the test committee.
pub fn test_authority_configs() -> NetworkConfig {
    let config_dir = tempfile::tempdir().unwrap().into_path();
    let rng = StdRng::from_seed([0; 32]);
    let mut configs = NetworkConfig::generate_with_rng(&config_dir, TEST_COMMITTEE_SIZE, rng);
    for config in configs.validator_configs.iter_mut() {
        let parameters = &mut config.consensus_config.narwhal_config;
        // NOTE: the following parameters are important to ensure tests run fast. Using the default
        // Narwhal parameters may result in tests taking >60 seconds.
        parameters.header_size = 1;
        parameters.max_header_delay = Duration::from_millis(200);
        parameters.batch_size = 1;
        parameters.max_batch_delay = Duration::from_millis(200);
    }
    configs
}

/// Spawn all authorities in the test committee into a separate tokio task.
pub async fn spawn_test_authorities<I>(
    objects: I,
    config: &NetworkConfig,
) -> Vec<AuthorityServerHandle>
where
    I: IntoIterator<Item = Object> + Clone,
{
    let mut handles = Vec::new();
    for validator in config.validator_configs() {
        let state = AuthorityState::new(
            validator.committee_config().committee(),
            validator.public_key(),
            Arc::pin(validator.key_pair().copy()),
            Arc::new(test_authority_store()),
            None,
            &sui_config::genesis::Genesis::get_default_genesis(),
        )
        .await;

        for o in objects.clone() {
            state.insert_genesis_object(o).await
        }

        let handle = make_authority(validator, state)
            .await
            .unwrap()
            .spawn()
            .await
            .unwrap();
        handles.push(handle);
    }
    handles
}

pub fn create_authority_aggregator(authority_configs: &[ValidatorInfo]) -> AuthorityAggregator {
    let voting_rights: BTreeMap<_, _> = authority_configs
        .iter()
        .map(|config| (config.public_key(), config.stake()))
        .collect();
    let committee = Committee::new(0, voting_rights);
    let clients: BTreeMap<_, AuthorityClient> = authority_configs
        .iter()
        .map(|config| {
            let client: AuthorityClient = Arc::new(SafeClient::new(
                Arc::new(NetworkAuthorityClient::connect_lazy(config.network_address()).unwrap()),
                committee.clone(),
                config.public_key(),
            ));
            (config.public_key(), client)
        })
        .collect();
    AuthorityAggregator::new(committee, clients)
}
