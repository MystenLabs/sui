// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Verifies the `disable_json_rpc` node config flag: it turns off the JSON-RPC
//! HTTP service while leaving the gRPC/REST service served on the same address
//! enabled. JSON-RPC is enabled by default, so disabling it is strictly
//! opt-in.

use std::time::Duration;

use jsonrpsee::core::client::ClientT;
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::rpc_params;
use rand::rngs::OsRng;
use sui_macros::sim_test;
use sui_rpc_api::Client;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn disabling_json_rpc_keeps_grpc_enabled() {
    let mut test_cluster = TestClusterBuilder::new().build().await;

    // The cluster's default fullnode keeps JSON-RPC enabled and acts as a
    // control: the same request must succeed there.
    let enabled_url = test_cluster.rpc_url().to_owned();

    // Spawn an additional fullnode with the JSON-RPC service disabled. We spawn
    // it directly through the swarm rather than through
    // `start_fullnode_from_config`, because the latter eagerly builds a JSON-RPC
    // client to handshake with the node, which would fail when the service is
    // turned off.
    let config = test_cluster
        .fullnode_config_builder()
        .with_disable_json_rpc(true)
        .build(&mut OsRng, test_cluster.swarm.config());
    assert!(config.disable_json_rpc);
    let rpc_url = format!("http://{}", config.json_rpc_address);
    let _handle = test_cluster.swarm.spawn_new_node(config).await;

    // gRPC is served on the same address and must stay available even though
    // JSON-RPC is disabled. The node is freshly spawned and still syncing, so we
    // poll until it has caught up enough to answer; a successful response proves
    // the gRPC service is enabled and reachable.
    let grpc_client = Client::new(&rpc_url).unwrap();
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if grpc_client.get_chain_identifier().await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    })
    .await
    .expect("grpc service should be enabled and reachable when json-rpc is disabled");

    // JSON-RPC requests to the disabled node must fail: the endpoints are never
    // registered, so there is nothing to answer the request.
    let disabled_client = HttpClientBuilder::default().build(&rpc_url).unwrap();
    let disabled_resp: Result<String, _> = disabled_client
        .request("sui_getChainIdentifier", rpc_params![])
        .await;
    assert!(
        disabled_resp.is_err(),
        "json-rpc should be disabled, but the request succeeded: {disabled_resp:?}"
    );

    // Control: the same JSON-RPC request succeeds against the default fullnode,
    // confirming the request itself is well-formed and that disabling is opt-in.
    let enabled_client = HttpClientBuilder::default().build(&enabled_url).unwrap();
    let enabled_resp: Result<String, _> = enabled_client
        .request("sui_getChainIdentifier", rpc_params![])
        .await;
    assert!(
        enabled_resp.is_ok(),
        "json-rpc should be enabled on the default fullnode, got: {enabled_resp:?}"
    );
}
