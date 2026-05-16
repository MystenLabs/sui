// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Scans a range of mainnet checkpoints from GCS, reporting every transaction
//! where the signature order does not match [sender, gas_owner].
//!
//! Output is JSONL: mismatch records followed by a final summary record.
//! When OUTPUT_GCS_BUCKET is set the file is uploaded to GCS at the end.
//!
//! Auth: workload identity on GKE, or ADC locally
//!       (gcloud auth application-default login).
//!
//! Env vars:
//!   GCS_BUCKET           source bucket (default: mysten-mainnet-checkpoints)
//!   LATEST_CHECKPOINT    tip sequence number (default: 276_015_239)
//!   START_CHECKPOINT     first checkpoint inclusive (default: LATEST - 100_000_000)
//!   END_CHECKPOINT       last checkpoint inclusive  (default: LATEST_CHECKPOINT)
//!   TEST                 set to "1" to scan only START..START+10_000
//!   CONCURRENCY          parallel fetch concurrency (default: 300)
//!   OUTPUT_FILE          local output path (default: /data/mismatches.jsonl)
//!   OUTPUT_GCS_BUCKET    if set, upload output here when done
//!   OUTPUT_GCS_KEY       object key inside OUTPUT_GCS_BUCKET
//!                        (default: sig-order-scan/results.jsonl)

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use object_store::ObjectStoreExt;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::path::Path as ObjPath;
use serde::Serialize;
use sui_data_ingestion_core::ReaderOptions;
use sui_data_ingestion_core::Worker;
use sui_data_ingestion_core::setup_single_workflow_with_options;
use sui_types::base_types::SuiAddress;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

// ── output record types ────────────────────────────────────────────────────

#[derive(Serialize)]
struct MismatchRecord<'a> {
    #[serde(rename = "type")]
    record_type: &'a str,
    checkpoint: u64,
    tx_digest: String,
    sender: String,
    gas_owner: String,
    sig0_address: String,
    sig1_address: String,
}

#[derive(Serialize)]
struct SummaryRecord<'a> {
    #[serde(rename = "type")]
    record_type: &'a str,
    total_checkpoints_processed: u64,
    total_mismatches: u64,
    start_checkpoint: u64,
    end_checkpoint: u64,
}

// ── worker ─────────────────────────────────────────────────────────────────

struct GcsScanWorker {
    output: Arc<Mutex<BufWriter<File>>>,
    checkpoints_processed: Arc<AtomicU64>,
    mismatches_found: Arc<AtomicU64>,
}

#[async_trait]
impl Worker for GcsScanWorker {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> Result<()> {
        let seq = checkpoint.checkpoint_summary.sequence_number;
        let processed = self.checkpoints_processed.fetch_add(1, Ordering::Relaxed) + 1;

        if processed % 10_000 == 0 {
            eprintln!(
                "progress checkpoint={seq} scanned={processed} mismatches={}",
                self.mismatches_found.load(Ordering::Relaxed),
            );
        }

        for ctx in &checkpoint.transactions {
            let data = ctx.transaction.data();
            let tx_data = data.transaction_data();

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

            let required = [sender, gas_owner];
            let actual: Vec<SuiAddress> = sigs
                .iter()
                .take(2)
                .map(SuiAddress::try_from)
                .collect::<Result<_, _>>()?;

            if actual[0] != required[0] || actual[1] != required[1] {
                let record = MismatchRecord {
                    record_type: "mismatch",
                    checkpoint: seq,
                    tx_digest: ctx.transaction.digest().to_string(),
                    sender: required[0].to_string(),
                    gas_owner: required[1].to_string(),
                    sig0_address: actual[0].to_string(),
                    sig1_address: actual[1].to_string(),
                };
                self.mismatches_found.fetch_add(1, Ordering::Relaxed);
                let line = serde_json::to_string(&record)?;
                let mut out = self.output.lock().unwrap();
                writeln!(out, "{line}")?;
            }
        }
        Ok(())
    }
}

// ── main ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let src_bucket =
        env::var("GCS_BUCKET").unwrap_or_else(|_| "mysten-mainnet-checkpoints".to_string());
    let remote_store_url = format!("gs://{src_bucket}");

    let latest: u64 = env::var("LATEST_CHECKPOINT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(276_015_239);

    let start: u64 = env::var("START_CHECKPOINT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| latest.saturating_sub(100_000_000));

    let test_mode = env::var("TEST").map(|v| v == "1").unwrap_or(false);

    let end: u64 = if test_mode {
        start + 10_000
    } else {
        env::var("END_CHECKPOINT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(latest)
    };

    let concurrency: usize = env::var("CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    let local_output =
        env::var("OUTPUT_FILE").unwrap_or_else(|_| "/data/mismatches.jsonl".to_string());
    let output_gcs_bucket = env::var("OUTPUT_GCS_BUCKET").ok();
    let output_gcs_key =
        env::var("OUTPUT_GCS_KEY").unwrap_or_else(|_| "sig-order-scan/results.jsonl".to_string());

    eprintln!(
        "scanning gs://{src_bucket} checkpoints {start}..{end} ({} total) concurrency={concurrency}{}",
        end.saturating_sub(start),
        if test_mode {
            " [TEST MODE — 10k]"
        } else {
            ""
        },
    );
    eprintln!("local output: {local_output}");
    if let Some(ref b) = output_gcs_bucket {
        eprintln!("gcs output: gs://{b}/{output_gcs_key}");
    }

    let file = File::create(&local_output)?;
    let output = Arc::new(Mutex::new(BufWriter::new(file)));
    let checkpoints_processed = Arc::new(AtomicU64::new(0));
    let mismatches_found = Arc::new(AtomicU64::new(0));

    let worker = GcsScanWorker {
        output: output.clone(),
        checkpoints_processed: checkpoints_processed.clone(),
        mismatches_found: mismatches_found.clone(),
    };

    let reader_options = ReaderOptions {
        batch_size: 500,
        timeout_secs: 30,
        upper_limit: Some(end),
        ..ReaderOptions::default()
    };

    let (executor, _term_sender) = setup_single_workflow_with_options(
        worker,
        remote_store_url,
        vec![],
        start,
        concurrency,
        Some(reader_options),
    )
    .await?;

    executor.await?;

    let total_checkpoints = checkpoints_processed.load(Ordering::Relaxed);
    let total_mismatches = mismatches_found.load(Ordering::Relaxed);

    // Write summary as the final JSONL line so the file is never empty.
    {
        let summary = SummaryRecord {
            record_type: "summary",
            total_checkpoints_processed: total_checkpoints,
            total_mismatches,
            start_checkpoint: start,
            end_checkpoint: end,
        };
        let mut out = output.lock().unwrap();
        writeln!(out, "{}", serde_json::to_string(&summary)?)?;
        out.flush()?;
    }

    eprintln!(
        "done: processed={total_checkpoints} mismatches={total_mismatches} output={local_output}",
    );

    if let Some(bucket) = output_gcs_bucket {
        eprintln!("uploading to gs://{bucket}/{output_gcs_key} …");
        upload_to_gcs(&local_output, &bucket, &output_gcs_key).await?;
        eprintln!("upload complete");
        eprintln!("download with: gsutil cp gs://{bucket}/{output_gcs_key} ./mismatches.jsonl");
    }

    Ok(())
}

async fn upload_to_gcs(local_path: &str, bucket: &str, key: &str) -> Result<()> {
    let store = GoogleCloudStorageBuilder::new()
        .with_bucket_name(bucket)
        .build()?;
    let data = tokio::fs::read(local_path).await?;
    store
        .put(&ObjPath::from(key), Bytes::from(data).into())
        .await?;
    Ok(())
}
