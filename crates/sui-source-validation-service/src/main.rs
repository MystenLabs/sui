// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_source_validation_service::{serve, verify_package};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let hardcoded_path = "../../crates/sui-framework/packages/sui-framework";
    verify_package(hardcoded_path).await.unwrap();
    serve()?.await.map_err(anyhow::Error::from)
}
