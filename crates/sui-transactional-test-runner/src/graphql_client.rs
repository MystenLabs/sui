// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use sui_graphql_rpc::client::simple_client::SimpleClient;
use tracing::info;

/// Generic polling function for GraphQL queries
async fn poll_until_condition<F>(
    client: &SimpleClient,
    query: String,
    timeout_msg: &str,
    base_timeout: Duration,
    scale: u64,
    check_condition: F,
) where
    F: Fn(&serde_json::Value) -> bool,
{
    let timeout = base_timeout.mul_f64(scale.max(1) as f64);

    tokio::time::timeout(timeout, async {
        loop {
            let resp = client
                .execute_to_graphql(query.to_string(), false, vec![], vec![])
                .await
                .unwrap()
                .response_body_json();

            if check_condition(&resp) {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
    .await
    .expect(timeout_msg);
}

pub async fn wait_for_checkpoint_catchup(
    client: &SimpleClient,
    checkpoint: u64,
    base_timeout: Duration,
) {
    info!(
        "Waiting for graphql to catchup to checkpoint {}, base time out is {}",
        checkpoint,
        base_timeout.as_secs()
    );

    let query = r#"
    {
        availableRange {
            last {
                sequenceNumber
            }
        }
    }"#
    .to_string();

    poll_until_condition(
        client,
        query,
        "Timeout waiting for graphql to catchup to checkpoint",
        base_timeout,
        checkpoint,
        |resp| {
            let current_checkpoint = resp["data"]["availableRange"]["last"].get("sequenceNumber");
            current_checkpoint
                .and_then(|cp| cp.as_u64())
                .map_or(false, |cp| cp >= checkpoint)
        },
    )
    .await;
}

pub async fn wait_for_epoch_catchup(client: &SimpleClient, epoch: u64, base_timeout: Duration) {
    info!(
        "Waiting for graphql to catchup to epoch {}, base time out is {}",
        epoch,
        base_timeout.as_secs()
    );

    let query = r#"
    {
        epoch {
            epochId
        }
    }"#
    .to_string();

    poll_until_condition(
        client,
        query,
        "Timeout waiting for graphql to catchup to epoch",
        base_timeout,
        epoch,
        |resp| {
            let latest_epoch = resp["data"]["epoch"].get("epochId");
            latest_epoch
                .and_then(|e| e.as_u64())
                .map_or(false, |e| e >= epoch)
        },
    )
    .await;
}

pub async fn wait_for_pruned_checkpoint(
    client: &SimpleClient,
    checkpoint: u64,
    base_timeout: Duration,
) {
    info!(
        "Waiting for checkpoint to be pruned {}, base time out is {}",
        checkpoint,
        base_timeout.as_secs()
    );

    let query = format!(
        r#"
        {{
            checkpoint(id: {{ sequenceNumber: {} }}) {{
                sequenceNumber
            }}
        }}"#,
        checkpoint
    );

    poll_until_condition(
        client,
        query,
        "Timeout waiting for checkpoint to be pruned",
        base_timeout,
        checkpoint,
        |resp| resp["data"]["checkpoint"].is_null(),
    )
    .await;
}
