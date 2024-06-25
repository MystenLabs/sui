// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_json_rpc_types::TransactionFilter;
use sui_json_rpc_types::{SuiTransactionBlockResponse, SuiTransactionBlockResponseQuery};
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::digests::TransactionDigest;
use sui_types::SUI_BRIDGE_OBJECT_ID;

use sui_bridge::retry_with_max_elapsed_time;
use tracing::{error, info};

const QUERY_DURATION: Duration = Duration::from_millis(500);

pub async fn start_sui_tx_polling_task(
    sui_client: SuiClient,
    mut cursor: Option<TransactionDigest>,
    tx: mysten_metrics::metered_channel::Sender<(Vec<SuiTransactionBlockResponse>, Option<TransactionDigest>)>,
) {
    info!("Starting SUI transaction polling task from {:?}", cursor);
    loop {
        let Ok(Ok(results)) = retry_with_max_elapsed_time!(
            sui_client.read_api().query_transaction_blocks(
                SuiTransactionBlockResponseQuery {
                    filter: Some(TransactionFilter::InputObject(SUI_BRIDGE_OBJECT_ID)),
                    options: Some(SuiTransactionBlockResponseOptions::full_content()),
                },
                cursor,
                None,
                false,
            ),
            Duration::from_secs(600)
        ) else {
            error!("Failed to query bridge transactions after retry");
            continue;
        };
        info!("Retrieved {} bridge transactions", results.data.len());
        tx.send((results.data, results.next_cursor))
            .await
            .expect("Failed to send transaction block to process");
        cursor = results.next_cursor;
        // tokio::time::sleep(QUERY_DURATION).await;
    }
}
