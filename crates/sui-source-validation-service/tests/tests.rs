// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use expect_test::expect;
use reqwest::Client;
use serde::Deserialize;

use sui_source_validation_service::{initialize, serve};

use test_utils::network::TestClusterBuilder;

#[derive(Deserialize)]
struct Response {
    source: String,
}

#[tokio::test]
async fn test_api_route() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await;
    let context = &mut cluster.wallet;

    initialize(context, vec![]).await?;
    tokio::spawn(serve().expect("Cannot start service."));

    let client = Client::new();
    let json = client
        .get("http://0.0.0.0:8000/api")
        .send()
        .await
        .expect("Request failed.")
        .json::<Response>()
        .await?;

    let expected = expect!["code"];
    expected.assert_eq(&json.source);
    Ok(())
}
