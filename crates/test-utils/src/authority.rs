// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::TEST_COMMITTEE_SIZE;
use prometheus::Registry;
use rand::{prelude::StdRng, SeedableRng};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use sui_config::{NetworkConfig, NodeConfig, ValidatorInfo};
use sui_core::authority_client::{AuthorityAPI, NetworkAuthorityClientMetrics};
use sui_core::epoch::epoch_store::EpochStore;
use sui_core::{
    authority_active::{
        checkpoint_driver::{CheckpointMetrics, CheckpointProcessControl},
        ActiveAuthority,
    },
    authority_aggregator::{AuthAggMetrics, AuthorityAggregator},
    authority_client::NetworkAuthorityClient,
    safe_client::SafeClientMetrics,
};
use sui_types::{committee::Committee, object::Object};

pub use sui_node::{SuiNode, SuiNodeHandle};
use sui_types::base_types::ObjectID;
use sui_types::messages::{ObjectInfoRequest, ObjectInfoRequestKind};

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

#[cfg(not(msim))]
pub async fn start_node(config: &NodeConfig, prom_registry: Registry) -> SuiNodeHandle {
    SuiNode::start(config, prom_registry).await.unwrap().into()
}

/// In the simulator, we call SuiNode::start from inside a newly spawned simulator node.
/// However, we then immmediately return the SuiNode handle back to the caller. The caller now has
/// a direct handle to an object that is "running" on a different "machine". By itself, this
/// doesn't break anything in the simulator, it just allows test code to magically mutate state
/// that is owned by some other machine.
///
/// Most of the time, tests do this just in order to verify some internal state, so this is fine
/// most of the time.
#[cfg(msim)]
pub async fn start_node(config: &NodeConfig, prom_registry: Registry) -> SuiNodeHandle {
    use std::net::{IpAddr, SocketAddr};

    let config = config.clone();
    let socket_addr = mysten_network::multiaddr::to_socket_addr(&config.network_address).unwrap();
    let ip = match socket_addr {
        SocketAddr::V4(v4) => IpAddr::V4(*v4.ip()),
        _ => panic!("unsupported protocol"),
    };

    let handle = sui_simulator::runtime::Handle::current();
    let builder = handle.create_node();
    let node = builder
        .ip(ip)
        .name(format!("{}", config.protocol_public_key()))
        .init(|| async {
            tracing::info!("node restarted");
        })
        .build();

    node.spawn(async move { SuiNode::start(&config, prom_registry).await.unwrap() })
        .await
        .unwrap()
        .into()
}

/// Spawn all authorities in the test committee into a separate tokio task.
pub async fn spawn_test_authorities<I>(objects: I, config: &NetworkConfig) -> Vec<SuiNodeHandle>
where
    I: IntoIterator<Item = Object> + Clone,
{
    let mut handles = Vec::new();
    for validator in config.validator_configs() {
        let prom_registry = Registry::new();
        let node = start_node(validator, prom_registry).await;
        let objects = objects.clone();

        node.with_async(|node| async move {
            let state = node.state();
            for o in objects {
                state.insert_genesis_object(o).await
            }
        })
        .await;

        handles.push(node);
    }
    handles
}

/// Spawn checkpoint processes with very short checkpointing intervals.
pub async fn spawn_checkpoint_processes(configs: &NetworkConfig, handles: &[SuiNodeHandle]) {
    // Start active part of each authority.
    for handle in handles {
        handle
            .with_async(|authority| async move {
                let state = authority.state();

                let aggregator =
                    test_authority_aggregator(configs, authority.state().epoch_store().clone());

                let inner_agg = aggregator.clone();
                let active_state = Arc::new(
                    ActiveAuthority::new_with_ephemeral_storage_for_test(state, inner_agg).unwrap(),
                );
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
            })
            .await;
    }
}

/// Create a test authority aggregator.
pub fn test_authority_aggregator(
    config: &NetworkConfig,
    epoch_store: Arc<EpochStore>,
) -> AuthorityAggregator<NetworkAuthorityClient> {
    let validators_info = config.validator_set();
    let committee = Committee::new(0, ValidatorInfo::voting_rights(validators_info)).unwrap();
    let clients: BTreeMap<_, _> = validators_info
        .iter()
        .map(|config| {
            (
                config.protocol_key(),
                NetworkAuthorityClient::connect_lazy(
                    config.network_address(),
                    Arc::new(NetworkAuthorityClientMetrics::new_for_tests()),
                )
                .unwrap(),
            )
        })
        .collect();
    let registry = prometheus::Registry::new();
    AuthorityAggregator::new(
        committee,
        epoch_store,
        clients,
        AuthAggMetrics::new(&registry),
        SafeClientMetrics::new(&registry),
    )
}

/// Get a network client to communicate with the consensus.
pub fn get_client(config: &ValidatorInfo) -> NetworkAuthorityClient {
    NetworkAuthorityClient::connect_lazy(
        config.network_address(),
        Arc::new(NetworkAuthorityClientMetrics::new_for_tests()),
    )
    .unwrap()
}

pub async fn get_object(config: &ValidatorInfo, object_id: ObjectID) -> Object {
    get_client(config)
        .handle_object_info_request(ObjectInfoRequest {
            object_id,
            request_kind: ObjectInfoRequestKind::LatestObjectInfo(None),
        })
        .await
        .unwrap()
        .object()
        .unwrap()
        .clone()
}
