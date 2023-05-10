// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::RegistryService;
use prometheus::Registry;
use rand::{prelude::StdRng, SeedableRng};
use std::num::NonZeroUsize;
use std::time::Duration;
use sui_config::NodeConfig;
use sui_core::authority_client::AuthorityAPI;
use sui_core::authority_client::NetworkAuthorityClient;
pub use sui_node::{SuiNode, SuiNodeHandle};
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::base_types::ObjectID;
use sui_types::messages_grpc::ObjectInfoRequest;
use sui_types::multiaddr::Multiaddr;
use sui_types::object::Object;

/// The default network buffer size of a test authority.
pub const NETWORK_BUFFER_SIZE: usize = 65_000;

/// Default committee size for tests
pub const TEST_COMMITTEE_SIZE: usize = 4;

/// Make an authority config for each of the `TEST_COMMITTEE_SIZE` authorities in the test committee.
pub fn test_authority_configs() -> NetworkConfig {
    test_and_configure_authority_configs(TEST_COMMITTEE_SIZE)
}

pub fn test_and_configure_authority_configs(committee_size: usize) -> NetworkConfig {
    let config_dir = tempfile::tempdir().unwrap().into_path();
    let rng = StdRng::from_seed([0; 32]);
    let mut configs = ConfigBuilder::new(config_dir)
        .committee_size(NonZeroUsize::new(committee_size).unwrap())
        .rng(rng)
        .build();
    for config in configs.validator_configs.iter_mut() {
        let parameters = &mut config.consensus_config.as_mut().unwrap().narwhal_config;
        // NOTE: the following parameters are important to ensure tests run fast. Using the default
        // Narwhal parameters may result in tests taking >60 seconds.
        parameters.header_num_of_batches_threshold = 1;
        parameters.max_header_delay = Duration::from_millis(200);
        parameters.min_header_delay = Duration::from_millis(200);
        parameters.batch_size = 1;
        parameters.max_batch_delay = Duration::from_millis(200);
    }
    configs
}

pub fn test_authority_configs_with_objects<I: IntoIterator<Item = Object> + Clone>(
    objects: I,
) -> (NetworkConfig, Vec<Object>) {
    test_and_configure_authority_configs_with_objects(TEST_COMMITTEE_SIZE, objects)
}

pub fn test_and_configure_authority_configs_with_objects<I: IntoIterator<Item = Object> + Clone>(
    committee_size: usize,
    objects: I,
) -> (NetworkConfig, Vec<Object>) {
    let config_dir = tempfile::tempdir().unwrap().into_path();
    let rng = StdRng::from_seed([0; 32]);
    let mut configs = ConfigBuilder::new(&config_dir)
        .rng(rng)
        .committee_size(committee_size.try_into().unwrap())
        .with_objects(objects.clone())
        .build();

    for config in configs.validator_configs.iter_mut() {
        let parameters = &mut config.consensus_config.as_mut().unwrap().narwhal_config;
        // NOTE: the following parameters are important to ensure tests run fast. Using the default
        // Narwhal parameters may result in tests taking >60 seconds.
        parameters.header_num_of_batches_threshold = 1;
        parameters.max_header_delay = Duration::from_millis(200);
        parameters.min_header_delay = Duration::from_millis(200);
        parameters.batch_size = 1;
        parameters.max_batch_delay = Duration::from_millis(200);
    }

    let objects = objects
        .into_iter()
        .map(|o| configs.genesis.object(o.id()).unwrap())
        .collect();

    (configs, objects)
}

#[cfg(not(msim))]
pub async fn start_node(config: &NodeConfig, registry_service: RegistryService) -> SuiNodeHandle {
    SuiNode::start(config, registry_service, None)
        .await
        .unwrap()
        .into()
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
pub async fn start_node(config: &NodeConfig, registry_service: RegistryService) -> SuiNodeHandle {
    use std::net::{IpAddr, SocketAddr};

    let config = config.clone();
    let socket_addr = config.network_address.to_socket_addr().unwrap();
    let ip = match socket_addr {
        SocketAddr::V4(v4) => IpAddr::V4(*v4.ip()),
        _ => panic!("unsupported protocol"),
    };

    let handle = sui_simulator::runtime::Handle::current();
    let builder = handle.create_node();
    tracing::info!("starting new node with ip {:?}", ip);
    let node = builder
        .ip(ip)
        .name(format!("{:?}", config.protocol_public_key().concise()))
        .init(|| async {
            tracing::info!("node restarted");
        })
        .build();

    let mut ret: SuiNodeHandle = node
        .spawn(async move {
            SuiNode::start(&config, registry_service, None)
                .await
                .unwrap()
        })
        .await
        .unwrap()
        .into();
    ret.shutdown_on_drop();
    ret
}

/// Spawn all authorities in the test committee into a separate tokio task.
pub async fn spawn_test_authorities(config: &NetworkConfig) -> Vec<SuiNodeHandle> {
    let mut handles = Vec::new();
    for validator in config.validator_configs() {
        let registry_service = RegistryService::new(Registry::new());
        let node = start_node(validator, registry_service).await;
        handles.push(node);
    }
    handles
}

/// Get a network client to communicate with the consensus.
pub fn get_client(net_address: &Multiaddr) -> NetworkAuthorityClient {
    NetworkAuthorityClient::connect_lazy(net_address).unwrap()
}

pub async fn get_object(net_address: &Multiaddr, object_id: ObjectID) -> Object {
    get_client(net_address)
        .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
            object_id, None,
        ))
        .await
        .unwrap()
        .object
}
