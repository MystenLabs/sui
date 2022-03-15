// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::anyhow;
use dropshot::test_util::{LogContext, TestContext};
use dropshot::{ConfigDropshot, ConfigLogging, ConfigLoggingLevel};
use futures::future::try_join_all;
use http::{Method, StatusCode};

use crate::{create_api, ServerContext};

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
        .map_err(|error| anyhow!("failed to create logger: {}", error))?;

    let documentation = api.openapi("Sui API", "0.1").json()?;

    let api_context = ServerContext::new(documentation);
    let testctx = TestContext::new(api, api_context, &config_dropshot, Some(logctx), log);

    testctx
        .client_testctx
        .make_request(
            Method::POST,
            "/sui/genesis",
            None as Option<()>,
            StatusCode::OK,
        )
        .await
        .expect("expected success");

    testctx
        .client_testctx
        .make_request(
            Method::POST,
            "/sui/start",
            None as Option<()>,
            StatusCode::OK,
        )
        .await
        .expect("expected success");

    let task = (0..10).map(|_| {
        testctx.client_testctx.make_request(
            Method::GET,
            "/addresses",
            None as Option<()>,
            StatusCode::OK,
        )
    });

    let task = task
        .into_iter()
        .map(|request| async { request.await })
        .collect::<Vec<_>>();

    let result = try_join_all(task).await.map_err(|e| anyhow!(e.message));

    // Clean up
    testctx
        .client_testctx
        .make_request(
            Method::POST,
            "/sui/stop",
            None as Option<()>,
            StatusCode::NO_CONTENT,
        )
        .await
        .expect("expected success");

    result?;

    Ok(())
}
