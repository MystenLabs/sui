// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Scans a range of mainnet checkpoints from GCS, reporting every transaction
//! where the signature order does not match [sender, gas_owner].
//!
//! Output is JSONL: per-transaction `mismatch` and `possible_alias` records
//! followed by a final `summary` record. When OUTPUT_GCS_BUCKET is set, the
//! file is uploaded to GCS at the end.
//!
//! Auth: workload identity on GKE, or ADC locally
//!       (gcloud auth application-default login).
//!
//! Env vars:
//!   GCS_BUCKET           source bucket (default: mysten-mainnet-checkpoints-use4)
//!   GCS_USER_PROJECT     `x-goog-user-project` header value for requester-pays
//!                        buckets. Default: fullnode-snapshot-gcs.
//!   START_CHECKPOINT     first checkpoint inclusive (required)
//!   END_CHECKPOINT       last checkpoint inclusive  (required)
//!   CONCURRENCY          deprecated — adaptive concurrency is used now,
//!                        kept for compatibility with the existing job spec.
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
use anyhow::anyhow;
use bytes::Bytes;
use object_store::ObjectStoreExt;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::path::Path as ObjPath;
use prometheus::Registry;
use serde::Serialize;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::ingestion::IngestionService;
use sui_indexer_alt_framework::ingestion::ingestion_client::CheckpointEnvelope;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;
use sui_types::base_types::SuiAddress;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionDataAPI;

#[derive(Serialize)]
struct MismatchRecord<'a> {
    #[serde(rename = "type")]
    record_type: &'a str,
    checkpoint: u64,
    timestamp_ms: u64,
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
    timestamp_ms: u64,
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

struct Counters {
    checkpoints_processed: AtomicU64,
    mismatches_found: AtomicU64,
    possible_aliases_found: AtomicU64,
}

/// Addresses a signature could authenticate. For zkLogin both the modern
/// unpadded derivation and the legacy padded derivation are accepted by the
/// protocol verifier (controlled by `verify_legacy_zklogin_address`), so we
/// include both. For every other authenticator the canonical address is the
/// only option.
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

fn process_checkpoint(
    checkpoint: &Checkpoint,
    counters: &Counters,
    output: &Mutex<BufWriter<File>>,
) -> Result<()> {
    let seq = checkpoint.summary.sequence_number;
    let timestamp_ms = checkpoint.summary.timestamp_ms;
    let processed = counters
        .checkpoints_processed
        .fetch_add(1, Ordering::Relaxed)
        + 1;

    if processed % 10_000 == 0 {
        eprintln!(
            "progress checkpoint={seq} scanned={processed} mismatches={} possible_aliases={}",
            counters.mismatches_found.load(Ordering::Relaxed),
            counters.possible_aliases_found.load(Ordering::Relaxed),
        );
    }

    for tx in &checkpoint.transactions {
        let tx_data = &tx.transaction;

        if tx_data.is_system_tx() {
            continue;
        }
        let sender = tx_data.sender();
        let gas_owner = tx_data.gas_owner();

        // Single required signer ⇒ no ordering to check.
        if sender == gas_owner {
            continue;
        }

        if tx.signatures.len() < 2 {
            continue;
        }

        // A parse failure shouldn't kill the whole checkpoint — treat it as
        // an empty acceptable set so the tx falls into the possible-alias
        // bucket and surfaces for audit.
        let sig0_addrs = acceptable_addresses(&tx.signatures[0]).unwrap_or_default();
        let sig1_addrs = acceptable_addresses(&tx.signatures[1]).unwrap_or_default();

        let sig0_matches_sender = sig0_addrs.contains(&sender);
        let sig0_matches_gas = sig0_addrs.contains(&gas_owner);
        let sig1_matches_sender = sig1_addrs.contains(&sender);
        let sig1_matches_gas = sig1_addrs.contains(&gas_owner);

        // Convention is [sender, gas_owner]; in-order is the happy path.
        if sig0_matches_sender && sig1_matches_gas {
            continue;
        }

        let digest = tx_data.digest().to_string();
        let line = if sig0_matches_gas && sig1_matches_sender {
            // Both sigs definitively match the required signer set, just
            // swapped — the case the scan is hunting.
            counters.mismatches_found.fetch_add(1, Ordering::Relaxed);
            let record = MismatchRecord {
                record_type: "mismatch",
                checkpoint: seq,
                timestamp_ms,
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
            counters
                .possible_aliases_found
                .fetch_add(1, Ordering::Relaxed);
            let record = PossibleAliasRecord {
                record_type: "possible_alias",
                checkpoint: seq,
                timestamp_ms,
                tx_digest: digest,
                sender: sender.to_string(),
                gas_owner: gas_owner.to_string(),
                sig0_addresses: sig0_addrs.iter().map(SuiAddress::to_string).collect(),
                sig1_addresses: sig1_addrs.iter().map(SuiAddress::to_string).collect(),
            };
            serde_json::to_string(&record)?
        };

        let mut out = output.lock().unwrap();
        writeln!(out, "{line}")?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let src_bucket =
        env::var("GCS_BUCKET").unwrap_or_else(|_| "mysten-mainnet-checkpoints-use4".to_string());
    let user_project =
        env::var("GCS_USER_PROJECT").unwrap_or_else(|_| "fullnode-snapshot-gcs".to_string());

    let start: u64 = env::var("START_CHECKPOINT")
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("START_CHECKPOINT must be set to a u64"))?;
    let end: u64 = env::var("END_CHECKPOINT")
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow!("END_CHECKPOINT must be set to a u64"))?;

    let local_output =
        env::var("OUTPUT_FILE").unwrap_or_else(|_| "/data/mismatches.jsonl".to_string());
    let output_gcs_bucket = env::var("OUTPUT_GCS_BUCKET").ok();
    let output_gcs_key =
        env::var("OUTPUT_GCS_KEY").unwrap_or_else(|_| "sig-order-scan/results.jsonl".to_string());

    eprintln!(
        "scanning gs://{src_bucket} checkpoints {start}..={end} ({} total) user_project={user_project}",
        (end - start) + 1,
    );
    eprintln!("local output: {local_output}");
    if let Some(ref b) = output_gcs_bucket {
        eprintln!("gcs output: gs://{b}/{output_gcs_key}");
    }

    let file = File::create(&local_output)?;
    let output = Arc::new(Mutex::new(BufWriter::new(file)));
    let counters = Arc::new(Counters {
        checkpoints_processed: AtomicU64::new(0),
        mismatches_found: AtomicU64::new(0),
        possible_aliases_found: AtomicU64::new(0),
    });

    let client_args = IngestionClientArgs {
        remote_store_gcs: Some(src_bucket.clone()),
        remote_store_headers: vec![(
            "x-goog-user-project"
                .parse()
                .map_err(|e| anyhow!("invalid header name: {e}"))?,
            user_project
                .parse()
                .map_err(|e| anyhow!("invalid header value: {e}"))?,
        )],
        ..Default::default()
    };
    let args = ClientArgs {
        ingestion: client_args,
        ..Default::default()
    };

    let registry = Registry::new();
    let mut service = IngestionService::new(args, IngestionConfig::default(), None, &registry)
        .map_err(|e| anyhow!("ingestion service init: {e}"))?;
    let mut rx = service.subscribe_bounded(256);
    let mut svc = service
        .run(start..=end)
        .await
        .map_err(|e| anyhow!("ingestion service run: {e}"))?;

    let consumer = {
        let counters = counters.clone();
        let output = output.clone();
        tokio::spawn(async move {
            while let Some(envelope) = rx.recv().await {
                let env: Arc<CheckpointEnvelope> = envelope;
                if let Err(e) = process_checkpoint(env.checkpoint.as_ref(), &counters, &output) {
                    eprintln!("error processing checkpoint: {e:?}");
                }
            }
            Ok::<(), anyhow::Error>(())
        })
    };

    svc.join().await.map_err(|e| anyhow!("svc join: {e}"))?;
    // Subscriber loop exits once the channel closes after the service shuts down.
    consumer.await??;

    let total_checkpoints = counters.checkpoints_processed.load(Ordering::Relaxed);
    let total_mismatches = counters.mismatches_found.load(Ordering::Relaxed);
    let total_possible_aliases = counters.possible_aliases_found.load(Ordering::Relaxed);

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
