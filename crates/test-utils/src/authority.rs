// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::TEST_COMMITTEE_SIZE;
use prometheus::Registry;
use rand::{prelude::StdRng, SeedableRng};
use std::sync::Arc;
use std::time::Duration;
use sui_config::{NetworkConfig, NodeConfig, ValidatorInfo};
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::authority_client::{AuthorityAPI, NetworkAuthorityClientMetrics};
use sui_types::object::Object;

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
        let parameters = &mut config.consensus_config.as_mut().unwrap().narwhal_config;
        // NOTE: the following parameters are important to ensure tests run fast. Using the default
        // Narwhal parameters may result in tests taking >60 seconds.
        parameters.header_num_of_batches_threshold = 1;
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
/// However, we then immediately return the SuiNode handle back to the caller. The caller now has
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
        .name(format!("{:?}", config.protocol_public_key().concise()))
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
