// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::sdk::Client;
use sui_rpc_api::client::Client as CoreClient;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_object() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let core_client = CoreClient::new(test_cluster.rpc_url()).unwrap();

    let _object = client.get_object("0x5".parse().unwrap()).await.unwrap();
    let _object = core_client
        .get_object("0x5".parse().unwrap())
        .await
        .unwrap();

    let _object = client
        .get_object_with_version("0x5".parse().unwrap(), 1)
        .await
        .unwrap();
    let _object = core_client
        .get_object_with_version("0x5".parse().unwrap(), 1.into())
        .await
        .unwrap();
}
