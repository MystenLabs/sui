// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::TEST_COMMITTEE_SIZE;
use rand::{prelude::StdRng, SeedableRng};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use sui_config::{NetworkConfig, ValidatorInfo};
use sui_core::{
    authority_active::{
        checkpoint_driver::{CheckpointMetrics, CheckpointProcessControl},
        ActiveAuthority,
    },
    authority_aggregator::{AuthAggMetrics, AuthorityAggregator},
    authority_client::NetworkAuthorityClient,
};
use sui_node::SuiNode;
use sui_types::{committee::Committee, object::Object};

/// The default network buffer size of a test authority.
pub const NETWORK_BUFFER_SIZE: usize = 65_000;

/// Make an authority config for each of the `TEST_COMMITTEE_SIZE` authorities in the test committee.
pub fn test_authority_configs() -> NetworkConfig {
    test_and_configure_authority_configs(TEST_COMMITTEE_SIZE)
}

pub fn test_and_configure_authority_configs(committee_size: usize) -> NetworkConfig {
    let config_dir = tempfile::tempdir().unwrap().into_path();
    let rng = StdRng::from_seed([0; 32]);
    let mut configs = NetworkConfig::generate_with_rng(&config_dir, committee_size, rng);
    for config in configs.validator_configs.iter_mut() {
        // Disable gossip by default to reduce non-determinism.
        // TODO: Once this library is more broadly used, we can make this a config argument.
        // Note: Enabling this will break checkpoint_catchup test, which needs a way to keep one
        // authority behind the others.
        config.enable_gossip = false;

        let parameters = &mut config.consensus_config.as_mut().unwrap().narwhal_config;
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
pub async fn spawn_test_authorities<I>(objects: I, config: &NetworkConfig) -> Vec<SuiNode>
where
    I: IntoIterator<Item = Object> + Clone,
{
    let mut handles = Vec::new();
    for validator in config.validator_configs() {
        let node = SuiNode::start(validator).await.unwrap();
        let state = node.state();

        for o in objects.clone() {
            state.insert_genesis_object(o).await
        }

        handles.push(node);
    }
    handles
}

/// Spawn checkpoint processes with very short checkpointing intervals.
pub async fn spawn_checkpoint_processes(
    aggregator: &AuthorityAggregator<NetworkAuthorityClient>,
    handles: &[SuiNode],
) {
    // Start active part of each authority.
    for authority in handles {
        let state = authority.state().clone();
        let inner_agg = aggregator.clone();
        let active_state =
            Arc::new(ActiveAuthority::new_with_ephemeral_storage(state, inner_agg).unwrap());
        let checkpoint_process_control = CheckpointProcessControl {
            long_pause_between_checkpoints: Duration::from_millis(10),
            ..CheckpointProcessControl::default()
        };
        let _active_authority_handle = active_state
            .spawn_checkpoint_process_with_config(
                checkpoint_process_control,
                CheckpointMetrics::new_for_tests(),
                false,
            )
            .await;
    }
}

/// Create a test authority aggregator.
pub fn test_authority_aggregator(
    config: &NetworkConfig,
) -> AuthorityAggregator<NetworkAuthorityClient> {
    let validators_info = config.validator_set();
    let committee = Committee::new(0, ValidatorInfo::voting_rights(validators_info)).unwrap();
    let clients: BTreeMap<_, _> = validators_info
        .iter()
        .map(|config| {
            (
                config.public_key(),
                NetworkAuthorityClient::connect_lazy(config.network_address()).unwrap(),
            )
        })
        .collect();
    let metrics = AuthAggMetrics::new(&prometheus::Registry::new());
    AuthorityAggregator::new(committee, clients, metrics)
}

/// Get a network client to communicate with the consensus.
pub fn get_client(config: &ValidatorInfo) -> NetworkAuthorityClient {
    NetworkAuthorityClient::connect_lazy(config.network_address()).unwrap()
}
