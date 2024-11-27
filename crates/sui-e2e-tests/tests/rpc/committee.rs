// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::sdk::Client;
use sui_sdk_types::types::ValidatorCommittee;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_committee() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();

    let _committee = client.get_committee(0).await.unwrap();
    let _committee = client.get_current_committee().await.unwrap();

    async fn raw_request(url: &str) {
        let client = reqwest::Client::new();

        // Make sure list works with json
        let _object = client
            .get(url)
            .header(reqwest::header::ACCEPT, sui_rpc_api::rest::APPLICATION_JSON)
            .send()
            .await
            .unwrap()
            .json::<ValidatorCommittee>()
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
        let _committee = bcs::from_bytes::<ValidatorCommittee>(&bytes).unwrap();
    }

    let url = format!("{}/v2/system/committee", test_cluster.rpc_url(),);

    raw_request(&url).await;

    let url = format!("{}/v2/system/committee/0", test_cluster.rpc_url());
    raw_request(&url).await;
}
