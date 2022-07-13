// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{messages::make_certificates, TEST_COMMITTEE_SIZE};
use rand::{prelude::StdRng, SeedableRng};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use sui_config::{NetworkConfig, ValidatorInfo};
use sui_core::authority_aggregator::AuthAggMetrics;
use sui_core::{
    authority_active::{checkpoint_driver::CheckpointProcessControl, ActiveAuthority},
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
    authority_client::NetworkAuthorityClient,
    gateway_state::GatewayMetrics,
};
use sui_node::SuiNode;
use sui_types::{
    committee::Committee,
    error::SuiResult,
    messages::{Transaction, TransactionInfoResponse},
    object::Object,
};

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
        let clients = aggregator.clone_inner_clients();
        let _active_authority_handle = tokio::spawn(async move {
            let active_state = Arc::new(
                ActiveAuthority::new_with_ephemeral_storage(
                    state,
                    clients,
                    GatewayMetrics::new_for_tests(),
                    CheckpointProcessControl {
                        long_pause_between_checkpoints: Duration::from_millis(10),
                        ..CheckpointProcessControl::default()
                    },
                )
                .unwrap(),
            );
            active_state.spawn_checkpoint_process().await
        });
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

/// Submit a certificate containing only owned-objects to all authorities.
pub async fn submit_single_owner_transaction(
    transaction: Transaction,
    configs: &[ValidatorInfo],
) -> Vec<TransactionInfoResponse> {
    let certificate = make_certificates(vec![transaction]).pop().unwrap();

    let mut responses = Vec::new();
    for config in configs {
        let client = get_client(config);
        let reply = client
            .handle_certificate(certificate.clone())
            .await
            .unwrap();
        responses.push(reply);
    }
    responses
}

/// Keep submitting the certificates of a shared-object transaction until it is sequenced by
/// at least one consensus node. We use the loop since some consensus protocols (like Tusk)
/// may drop transactions. The certificate is submitted to every Sui authority.
pub async fn submit_shared_object_transaction(
    transaction: Transaction,
    configs: &[ValidatorInfo],
) -> Vec<SuiResult<TransactionInfoResponse>> {
    let certificate = make_certificates(vec![transaction]).pop().unwrap();

    loop {
        let futures: Vec<_> = configs
            .iter()
            .map(|config| {
                let client = get_client(config);
                let cert = certificate.clone();
                async move { client.handle_certificate(cert).await }
            })
            .collect();

        let replies: Vec<_> = futures::future::join_all(futures)
            .await
            .into_iter()
            // Remove all `FailedToHearBackFromConsensus` replies. Note that the original Sui error type
            // `SuiError::FailedToHearBackFromConsensus(..)` is lost when the message is sent through the
            // network (it is replaced by `RpcError`). As a result, the following filter doesn't work:
            // `.filter(|result| !matches!(result, Err(SuiError::FailedToHearBackFromConsensus(..))))`.
            .filter(|result| match result {
                Err(e) => !e.to_string().contains("deadline has elapsed"),
                _ => true,
            })
            .collect();

        if !replies.is_empty() {
            break replies;
        }
    }
}
