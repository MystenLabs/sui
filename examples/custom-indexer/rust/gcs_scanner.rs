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
use sui_types::signature::GenericSignature;
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

/// Either sig fails to match either required signer (sender / gas_owner). The
/// signer may have on-chain address aliases (post-protocol-v116) that we can't
/// resolve without object-store access — or the tx may be using an
/// authenticator we don't fully understand. Logged so we can audit separately.
#[derive(Serialize)]
struct PossibleAliasRecord<'a> {
    #[serde(rename = "type")]
    record_type: &'a str,
    checkpoint: u64,
    tx_digest: String,
    sender: String,
    gas_owner: String,
    sig0_addresses: Vec<String>,
    sig1_addresses: Vec<String>,
}

#[derive(Serialize)]
struct SummaryRecord<'a> {
    #[serde(rename = "type")]
    record_type: &'a str,
    total_checkpoints_processed: u64,
    total_mismatches: u64,
    total_possible_aliases: u64,
    start_checkpoint: u64,
    end_checkpoint: u64,
}

// ── worker ─────────────────────────────────────────────────────────────────

struct GcsScanWorker {
    output: Arc<Mutex<BufWriter<File>>>,
    checkpoints_processed: Arc<AtomicU64>,
    mismatches_found: Arc<AtomicU64>,
    possible_aliases_found: Arc<AtomicU64>,
}

/// Addresses a signature could authenticate. For zkLogin, both the modern
/// unpadded derivation and the legacy padded derivation are accepted by the
/// verifier (controlled by `verify_legacy_zklogin_address`), so we include
/// both. For every other authenticator the canonical address is the only
/// option.
fn acceptable_addresses(sig: &GenericSignature) -> Result<Vec<SuiAddress>> {
    let mut addrs = vec![SuiAddress::try_from(sig)?];
    if let GenericSignature::ZkLoginAuthenticator(z) = sig {
        if let Ok(padded) = SuiAddress::try_from_padded(&z.inputs) {
            if !addrs.contains(&padded) {
                addrs.push(padded);
            }
        }
    }
    Ok(addrs)
}

#[async_trait]
impl Worker for GcsScanWorker {
    type Result = ();

    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> Result<()> {
        let seq = checkpoint.checkpoint_summary.sequence_number;
        let processed = self.checkpoints_processed.fetch_add(1, Ordering::Relaxed) + 1;

        if processed % 10_000 == 0 {
            eprintln!(
                "progress checkpoint={seq} scanned={processed} mismatches={} possible_aliases={}",
                self.mismatches_found.load(Ordering::Relaxed),
                self.possible_aliases_found.load(Ordering::Relaxed),
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

            // A sig parsing failure shouldn't kill the whole checkpoint —
            // treat it as "no acceptable addresses" so it falls through to the
            // possible-alias branch and gets surfaced for audit.
            let sig0_addrs = acceptable_addresses(&sigs[0]).unwrap_or_default();
            let sig1_addrs = acceptable_addresses(&sigs[1]).unwrap_or_default();

            let sig0_matches_sender = sig0_addrs.contains(&sender);
            let sig0_matches_gas = sig0_addrs.contains(&gas_owner);
            let sig1_matches_sender = sig1_addrs.contains(&sender);
            let sig1_matches_gas = sig1_addrs.contains(&gas_owner);

            // Convention is [sender, gas_owner]; in-order is the happy path.
            if sig0_matches_sender && sig1_matches_gas {
                continue;
            }

            let digest = ctx.transaction.digest().to_string();
            let line = if sig0_matches_gas && sig1_matches_sender {
                // Both sigs definitively match the required signer set, just
                // swapped — the case the scan is hunting.
                self.mismatches_found.fetch_add(1, Ordering::Relaxed);
                let record = MismatchRecord {
                    record_type: "mismatch",
                    checkpoint: seq,
                    tx_digest: digest,
                    sender: sender.to_string(),
                    gas_owner: gas_owner.to_string(),
                    sig0_address: sig0_addrs
                        .first()
                        .map(|a| a.to_string())
                        .unwrap_or_default(),
                    sig1_address: sig1_addrs
                        .first()
                        .map(|a| a.to_string())
                        .unwrap_or_default(),
                };
                serde_json::to_string(&record)?
            } else {
                // At least one sig doesn't match either required signer —
                // could be an aliased address, an exotic authenticator, or a
                // sig we couldn't decode.
                self.possible_aliases_found
                    .fetch_add(1, Ordering::Relaxed);
                let record = PossibleAliasRecord {
                    record_type: "possible_alias",
                    checkpoint: seq,
                    tx_digest: digest,
                    sender: sender.to_string(),
                    gas_owner: gas_owner.to_string(),
                    sig0_addresses: sig0_addrs.iter().map(SuiAddress::to_string).collect(),
                    sig1_addresses: sig1_addrs.iter().map(SuiAddress::to_string).collect(),
                };
                serde_json::to_string(&record)?
            };

            let mut out = self.output.lock().unwrap();
            writeln!(out, "{line}")?;
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
    let possible_aliases_found = Arc::new(AtomicU64::new(0));

    let worker = GcsScanWorker {
        output: output.clone(),
        checkpoints_processed: checkpoints_processed.clone(),
        mismatches_found: mismatches_found.clone(),
        possible_aliases_found: possible_aliases_found.clone(),
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
    let total_possible_aliases = possible_aliases_found.load(Ordering::Relaxed);

    // Write summary as the final JSONL line so the file is never empty.
    {
        let summary = SummaryRecord {
            record_type: "summary",
            total_checkpoints_processed: total_checkpoints,
            total_mismatches,
            total_possible_aliases,
            start_checkpoint: start,
            end_checkpoint: end,
        };
        let mut out = output.lock().unwrap();
        writeln!(out, "{}", serde_json::to_string(&summary)?)?;
        out.flush()?;
    }

    eprintln!(
        "done: processed={total_checkpoints} mismatches={total_mismatches} possible_aliases={total_possible_aliases} output={local_output}",
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
