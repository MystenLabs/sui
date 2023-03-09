// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use rand::Rng;
use sui_indexer::new_rpc_client;
use sui_json_rpc_types::{CheckpointId, SuiTransactionResponseOptions};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    info!("Running correctness check for indexer...");
    let test_config = TestConfig::parse();
    let fn_rpc_client = new_rpc_client(&test_config.fn_rpc_client_url).await?;
    let indexer_rpc_client = new_rpc_client(&test_config.indexer_rpc_client_url).await?;

    let latest_checkpoint = indexer_rpc_client
        .read_api()
        .get_latest_checkpoint_sequence_number()
        .await?;

    let num = rand::thread_rng().gen_range(10..100);

    let target_checkpoint = if latest_checkpoint - num > 0 {
        latest_checkpoint - num
    } else {
        1
    };

    let fn_checkpoint = fn_rpc_client
        .read_api()
        .get_checkpoint(CheckpointId::SequenceNumber(target_checkpoint))
        .await?;

    let indexer_checkpoint = indexer_rpc_client
        .read_api()
        .get_checkpoint(CheckpointId::SequenceNumber(target_checkpoint))
        .await?;

    assert_eq!(
        fn_checkpoint.transactions.len(),
        indexer_checkpoint.transactions.len(),
        "Checkpoint number {} length is not the same for FN and Indexer",
        target_checkpoint
    );

    let fn_checkpoint_transactions = fn_checkpoint.transactions;
    let indexer_checkpoint_transactions = indexer_checkpoint.transactions;

    for i in 0..fn_checkpoint_transactions.len() {
        let fn_txn_digest = fn_checkpoint_transactions.get(i).cloned();
        let idx_txn_digest = indexer_checkpoint_transactions.get(i).cloned();
        assert_eq!(
            fn_txn_digest, idx_txn_digest,
            "Checkpoint transactions mismatch found in {}",
            target_checkpoint
        );

        if let (Some(fn_txn_digest), Some(idx_txn_digest)) = (fn_txn_digest, idx_txn_digest) {
            let fetch_options = SuiTransactionResponseOptions::full_content();
            let fn_sui_txn_response = fn_rpc_client
                .read_api()
                .get_transaction_with_options(fn_txn_digest, fetch_options.clone())
                .await?;
            let indexer_sui_txn_response = indexer_rpc_client
                .read_api()
                .get_transaction_with_options(idx_txn_digest, fetch_options)
                .await?;
            assert_eq!(
                fn_sui_txn_response, indexer_sui_txn_response,
                "Checkpoint transactions mismatch found in {}",
                target_checkpoint
            );
        }
    }

    Ok(())
}

#[derive(Parser)]
#[clap(name = "Transactions Test")]
pub struct TestConfig {
    #[clap(long)]
    pub fn_rpc_client_url: String,
    #[clap(long)]
    pub indexer_rpc_client_url: String,
}
