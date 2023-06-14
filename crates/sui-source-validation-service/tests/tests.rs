// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use expect_test::expect;
use reqwest::Client;
use serde::Deserialize;

use sui_source_validation_service::{serve, verify_package};

#[derive(Deserialize)]
struct Response {
    source: String,
}

#[tokio::test]
async fn test_index_route() -> anyhow::Result<()> {
    let hardcoded_path = "../../crates/sui-framework/packages/sui-framework";
    verify_package(hardcoded_path)
        .await
        .expect("Could not verify");
    tokio::spawn(serve().expect("Cannot start service."));

    let client = Client::new();
    let json = client
        .get("http://0.0.0.0:8000")
        .send()
        .await
        .expect("Request failed.")
        .json::<Response>()
        .await?;

    let expected = expect!["code"];
    expected.assert_eq(&json.source);
    Ok(())
}
