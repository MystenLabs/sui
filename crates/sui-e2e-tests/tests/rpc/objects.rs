// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::sdk::Client;
use sui_rpc_api::client::Client as CoreClient;
use sui_rpc_api::ObjectResponse;
use sui_sdk_types::types::Object;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_object() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let core_client = CoreClient::new(test_cluster.rpc_url());

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

    async fn raw_request(url: &str) {
        let client = reqwest::Client::new();

        // Make sure list works with json
        let _object = client
            .get(url)
            .header(reqwest::header::ACCEPT, sui_rpc_api::rest::APPLICATION_JSON)
            .send()
            .await
            .unwrap()
            .json::<ObjectResponse>()
            .await
            .unwrap();

        // TODO remove this once the BCS format is no longer supported by the rest endpoint and clients
        // wanting binary have migrated to grpc
        let bytes = client
            .get(url)
            .header(reqwest::header::ACCEPT, sui_rpc_api::rest::APPLICATION_BCS)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let _object = bcs::from_bytes::<Object>(&bytes).unwrap();
    }

    let url = format!("{}/v2/objects/0x5", test_cluster.rpc_url());
    raw_request(&url).await;

    let url = format!("{}/v2/objects/0x5/version/1", test_cluster.rpc_url());
    raw_request(&url).await;
}
