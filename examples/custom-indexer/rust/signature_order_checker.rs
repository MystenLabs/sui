// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;

use anyhow::Result;
use async_trait::async_trait;
use sui_data_ingestion_core::Worker;
use sui_data_ingestion_core::setup_single_workflow;
use sui_types::base_types::SuiAddress;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

struct SignatureOrderWorker;

#[async_trait]
impl Worker for SignatureOrderWorker {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> Result<()> {
        let seq = checkpoint.checkpoint_summary.sequence_number;
        if seq % 1000 == 0 {
            eprintln!("progress: checkpoint {seq}");
        }
        for ctx in &checkpoint.transactions {
            let data = ctx.transaction.data();
            let tx_data = data.transaction_data();

            // Required signers are [sender, gas_owner] (gas_owner only if it differs from sender).
            // If sender == gas_owner there is exactly one required signer and nothing to reorder.
            if tx_data.is_system_tx() {
                continue;
            }
            let sender = tx_data.sender();
            let gas_owner = tx_data.gas_owner();
            if sender == gas_owner {
                continue;
            }

            let sigs = data.tx_signatures();
            if sigs.len() < 2 {
                continue;
            }

            // The protocol enforces tx_signatures.len() == required_signers.len(), so with
            // sender != gas_owner we expect exactly two signatures here.
            let required = [sender, gas_owner];
            let actual: Vec<SuiAddress> = sigs
                .iter()
                .take(2)
                .map(SuiAddress::try_from)
                .collect::<Result<_, _>>()?;

            if actual[0] != required[0] || actual[1] != required[1] {
                let digest = ctx.transaction.digest();
                println!(
                    "MISMATCH checkpoint={} tx={} required=[sender={}, gas_owner={}] signatures=[{}, {}]",
                    seq, digest, required[0], required[1], actual[0], actual[1],
                );
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let endpoint = env::var("CHECKPOINT_ENDPOINT")
        .unwrap_or_else(|_| "https://checkpoints.mainnet.sui.io".to_string());
    let start = env::var("START_CHECKPOINT")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(100_000_000);
    let concurrency = env::var("CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(50);

    eprintln!(
        "Scanning {endpoint} from checkpoint {start} with concurrency {concurrency} for out-of-order signatures",
    );

    let (executor, _term_sender) =
        setup_single_workflow(SignatureOrderWorker, endpoint, start, concurrency, None).await?;
    executor.await?;
    Ok(())
}
