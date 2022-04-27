// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::anyhow;
use dropshot::test_util::{LogContext, TestContext};
use dropshot::{ConfigDropshot, ConfigLogging, ConfigLoggingLevel};
use futures::future::try_join_all;
use http::{Method, StatusCode};
use sui::SUI_WALLET_CONFIG;

use sui::wallet_commands::WalletContext;

use crate::rest_server_tests::sui_network::start_test_network;
use crate::{create_api, ServerContext};

mod sui_network;

#[tokio::test]
async fn test_concurrency() -> Result<(), anyhow::Error> {
    let api = create_api();

    let config_dropshot: ConfigDropshot = Default::default();
    let log_config = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Debug,
    };
    let logctx = LogContext::new("test_name", &log_config);

    let log = log_config
        .to_logger("rest_server")
        .map_err(|error| anyhow!("failed to create logger: {error}"))?;

    // Start sui network
    let working_dir = tempfile::tempdir()?;
    let network = start_test_network(working_dir.path(), None, None).await?;
    let wallet = WalletContext::new(&working_dir.path().join(SUI_WALLET_CONFIG))?;
    let address = wallet.config.accounts.first().unwrap();
    let documentation = api.openapi("Sui API", "0.1").json()?;

    let api_context = ServerContext::new(documentation, wallet.gateway);
    let testctx = TestContext::new(api, api_context, &config_dropshot, Some(logctx), log);
    let url = format!("/api/objects?address={}", address);

    let task = (0..10).map(|_| {
        testctx
            .client_testctx
            .make_request_no_body(Method::GET, &url, StatusCode::OK)
    });

    let task = task
        .into_iter()
        .map(|request| async { request.await })
        .collect::<Vec<_>>();

    try_join_all(task).await.map_err(|e| anyhow!(e.message))?;

    network.kill().await?;
    Ok(())
}
