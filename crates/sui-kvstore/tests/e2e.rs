// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end integration tests for the BigTable KV Store.
//!
//! Each test spawns its own BigTable emulator process on a random port,
//! creates the required tables, and tears everything down when done.
//! Tests require `gcloud`, `cbt`, and the BigTable emulator on PATH.

use std::io::BufRead;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use futures::TryStreamExt;
use futures::future::try_join_all;
use prost_types::FieldMask;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_indexer_alt_framework::ingestion::streaming_client::StreamingClientArgs;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_keys::keystore::AccountKeystore;
use sui_kvstore::BigTableClient;
use sui_kvstore::BigTableIndexer;
use sui_kvstore::BigTableStore;
use sui_kvstore::KeyValueStoreReader;
use sui_kvstore::set_write_legacy_data;
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::Bcs;
use sui_rpc::proto::sui::rpc::v2::ExecuteTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::Transaction as GrpcTransaction;
use sui_rpc::proto::sui::rpc::v2::UserSignature;
use sui_types::base_types::SuiAddress;
use sui_types::message_envelope::Message;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use test_cluster::{TestCluster, TestClusterBuilder};
use tokio::process::Command as TokioCommand;
use tokio::time::interval;
use url::Url;

const INSTANCE_ID: &str = "bigtable_test_instance";
const TABLES: &[&str] = &[
    sui_kvstore::tables::objects::NAME,
    sui_kvstore::tables::transactions::NAME,
    sui_kvstore::tables::checkpoints::NAME,
    sui_kvstore::tables::checkpoints_by_digest::NAME,
    sui_kvstore::tables::watermark_alt_legacy::NAME,
    sui_kvstore::tables::epochs::NAME,
];

/// Resolve the path to `cbtemulator` relative to the gcloud SDK root.
/// Works regardless of whether gcloud was installed via apt, brew, or the standalone installer.
fn cbtemulator_path() -> PathBuf {
    let output = Command::new("gcloud")
        .args(["info", "--format=value(installation.sdk_root)"])
        .output()
        .expect("gcloud not found on PATH — install the Google Cloud SDK to run these tests");
    assert!(output.status.success(), "failed to query gcloud sdk root");

    let sdk_root = String::from_utf8(output.stdout)
        .expect("non-utf8 gcloud sdk root")
        .trim()
        .to_string();

    let path = PathBuf::from(sdk_root).join("platform/bigtable-emulator/cbtemulator");
    assert!(
        path.exists(),
        "cbtemulator not found at {path:?} — run: gcloud components install bigtable"
    );
    path
}

fn require_bigtable_emulator() {
    cbtemulator_path();
    assert!(
        Command::new("cbt").arg("-version").output().is_ok(),
        "cbt not found on PATH — run: gcloud components install cbt"
    );
}

/// A self-contained BigTable emulator process.
/// Spawns the emulator on a random port.
/// The emulator process is killed when this struct is dropped.
struct BigTableEmulator {
    child: Child,
    host: String,
    // Keep stdout open so the emulator doesn't get SIGPIPE and die.
    _stdout_drain: std::thread::JoinHandle<()>,
}

impl BigTableEmulator {
    fn start() -> Result<Self> {
        let mut child = Command::new(cbtemulator_path())
            .arg("-port=0")
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .context("Failed to spawn BigTable emulator")?;

        let stdout = child.stdout.take().expect("stdout was piped");
        let mut reader = std::io::BufReader::new(stdout);

        let mut host = None;
        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            let n = reader
                .read_line(&mut line_buf)
                .context("Failed to read emulator stderr")?;
            if n == 0 {
                break;
            }
            // cbtemulator prints: Cloud Bigtable emulator running on 127.0.0.1:PORT
            if line_buf.contains("Cloud Bigtable emulator running on") {
                if let Some(addr) = line_buf.rsplit("running on ").next() {
                    let addr = addr.trim();
                    if let Some(port) = addr.rsplit(':').next() {
                        host = Some(format!("localhost:{port}"));
                    }
                }
                break;
            }
        }

        let host = host.context("Failed to parse emulator host:port from stdout")?;

        // Drain remaining stdout in a background thread to prevent SIGPIPE.
        let stdout_drain = std::thread::spawn(move || {
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
        });

        Ok(Self {
            child,
            host,
            _stdout_drain: stdout_drain,
        })
    }

    fn host(&self) -> &str {
        &self.host
    }
}

impl Drop for BigTableEmulator {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Create all required BigTable tables in parallel using async subprocesses.
async fn create_tables(host: &str, instance_id: &str) -> Result<()> {
    let futures: Vec<_> = TABLES
        .iter()
        .map(|table| {
            let host = host.to_string();
            let instance_id = instance_id.to_string();
            let table = *table;
            async move {
                let output = TokioCommand::new("cbt")
                    .args(["-instance", &instance_id, "-project", "emulator"])
                    .arg("createtable")
                    .arg(table)
                    .env("BIGTABLE_EMULATOR_HOST", &host)
                    .output()
                    .await
                    .with_context(|| format!("Failed to run cbt createtable {table}"))?;
                if !output.status.success() {
                    bail!(
                        "cbt createtable {table} failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }

                let output = TokioCommand::new("cbt")
                    .args(["-instance", &instance_id, "-project", "emulator"])
                    .args(["createfamily", table, "sui"])
                    .env("BIGTABLE_EMULATOR_HOST", &host)
                    .output()
                    .await
                    .with_context(|| format!("Failed to run cbt createfamily {table}"))?;
                if !output.status.success() {
                    bail!(
                        "cbt createfamily {table} failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }

                Ok(())
            }
        })
        .collect();

    try_join_all(futures).await?;
    Ok(())
}

/// Get all coin objects for an address using gRPC list_owned_objects.
async fn get_all_coins(client: &mut GrpcClient, address: SuiAddress) -> Result<Vec<Object>> {
    let request = ListOwnedObjectsRequest::default()
        .with_owner(address.to_string())
        .with_object_type("0x2::coin::Coin".to_string())
        .with_read_mask(FieldMask {
            paths: vec!["bcs".to_string()],
        });

    let objects: Vec<Object> = client
        .list_owned_objects(request)
        .and_then(|grpc_object| async move {
            let bcs = grpc_object
                .bcs
                .as_ref()
                .ok_or_else(|| tonic::Status::internal("Missing BCS data in object"))?;
            bcs.deserialize::<Object>()
                .map_err(|e| tonic::Status::internal(format!("Failed to deserialize object: {e}")))
        })
        .try_collect()
        .await
        .context("Failed to list owned objects")?;

    Ok(objects)
}

/// Execute a signed transaction via gRPC and wait for it to land in a checkpoint.
async fn grpc_execute_transaction(client: &mut GrpcClient, signed_tx: &Transaction) -> Result<()> {
    let mut proto_tx = GrpcTransaction::default();
    proto_tx.bcs = Some(Bcs::serialize(signed_tx.transaction_data()).unwrap());

    let signatures = signed_tx
        .tx_signatures()
        .iter()
        .map(|s| {
            let mut sig = UserSignature::default();
            let mut bcs = Bcs::default();
            bcs.name = None;
            bcs.value = Some(s.as_ref().to_owned().into());
            sig.bcs = Some(bcs);
            sig
        })
        .collect();

    let exec_request = ExecuteTransactionRequest::default()
        .with_transaction(proto_tx)
        .with_signatures(signatures)
        .with_read_mask(FieldMask::from_paths(["*"]));

    client
        .execute_transaction_and_wait_for_checkpoint(exec_request, Duration::from_secs(20))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("Failed to execute transaction via gRPC")?;

    Ok(())
}

/// Test cluster that combines a real TestCluster (validators + fullnode) with
/// a BigTable emulator and indexer for e2e testing.
struct TestHarness {
    cluster: TestCluster,
    client: BigTableClient,
    grpc_client: GrpcClient,
    _emulator: BigTableEmulator,
}

impl TestHarness {
    async fn new() -> Result<Self> {
        require_bigtable_emulator();
        set_write_legacy_data(true);

        let emulator_future = async {
            let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
                .await
                .context("spawn_blocking panicked")??;
            create_tables(emulator.host(), INSTANCE_ID).await?;
            Ok::<_, anyhow::Error>(emulator)
        };

        let cluster_future =
            async { Ok::<_, anyhow::Error>(TestClusterBuilder::new().build().await) };

        let (emulator, cluster) = tokio::try_join!(emulator_future, cluster_future)?;

        let client =
            BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string())
                .await
                .context("Failed to create BigTable client")?;

        let store = BigTableStore::new(client.clone());
        let registry = prometheus::Registry::new();

        let indexer_args = IndexerArgs::default();
        let rpc_url = cluster.rpc_url();

        let grpc_client = GrpcClient::new(rpc_url).context("Failed to create gRPC client")?;

        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                rpc_api_url: Some(Url::parse(rpc_url).expect("Invalid RPC URL")),
                ..Default::default()
            },
            streaming: StreamingClientArgs {
                streaming_url: Some(rpc_url.parse().expect("Invalid streaming URI")),
            },
        };
        let ingestion_config = IngestionConfig::default();

        let bigtable_indexer = BigTableIndexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            ConcurrentConfig::default(),
            &registry,
        )
        .await
        .context("Failed to create BigTableIndexer")?;

        let mut service = bigtable_indexer
            .indexer
            .run()
            .await
            .context("Failed to run indexer")?;

        tokio::spawn(async move {
            let _ = service.join().await;
        });

        Ok(Self {
            cluster,
            client,
            grpc_client,
            _emulator: emulator,
        })
    }

    /// Build, sign, and execute a SUI transfer via gRPC.
    async fn transfer_sui(&mut self, recipient: SuiAddress, amount: u64) -> Result<Transaction> {
        let sender = self.cluster.get_address_0();
        let keystore = &self.cluster.wallet.config.keystore;

        let coins = get_all_coins(&mut self.grpc_client, sender).await?;
        let gas_object = coins
            .first()
            .context("No coins available for sender")?
            .compute_object_reference();

        let gas_price = self.cluster.get_reference_gas_price().await;
        let tx_data = TransactionData::new_transfer_sui(
            recipient,
            sender,
            Some(amount),
            gas_object,
            1_000_000,
            gas_price,
        );

        let signed_tx = to_sender_signed_transaction(tx_data, keystore.export(&sender)?);

        grpc_execute_transaction(&mut self.grpc_client, &signed_tx).await?;

        Ok(signed_tx)
    }

    async fn wait_for_watermark(&mut self, checkpoint: u64, epoch: u64) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(60), async {
            let mut interval = interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                let ok = self.client.get_watermark().await.is_ok_and(|wm| {
                    wm.is_some_and(|wm| {
                        wm.checkpoint_hi_inclusive >= checkpoint && wm.epoch_hi_inclusive >= epoch
                    })
                });
                if ok {
                    break;
                }
            }
        })
        .await
        .context("Timeout waiting for watermark to advance")
    }

    fn bigtable_client(&mut self) -> &mut BigTableClient {
        &mut self.client
    }
}

#[tokio::test]
async fn test_indexer_e2e() -> Result<()> {
    let mut harness = TestHarness::new().await?;

    // -- Execute 3 transfers --
    // execute_transaction_and_wait_for_checkpoint guarantees each txn is checkpointed
    // on the fullnode before returning.
    let mut signed_txns = Vec::new();
    for _ in 0..3 {
        let recipient = SuiAddress::random_for_testing_only();
        signed_txns.push(harness.transfer_sui(recipient, 1).await?);
    }
    let tx_digests: Vec<_> = signed_txns.iter().map(|tx| *tx.digest()).collect();

    // Look up checkpoint numbers via the fullnode gRPC API (available immediately
    // since execute_transaction_and_wait_for_checkpoint already waited).
    let mut tx_checkpoints = Vec::new();
    for digest in &tx_digests {
        let resp = harness
            .grpc_client
            .ledger_client()
            .get_transaction(
                GetTransactionRequest::default()
                    .with_digest(digest.to_string())
                    .with_read_mask(FieldMask::from_paths(["checkpoint"])),
            )
            .await
            .context("get_transaction RPC failed")?;
        let cp = resp
            .into_inner()
            .transaction
            .and_then(|t| t.checkpoint)
            .context("get_transaction response missing checkpoint")?;
        tx_checkpoints.push(cp);
    }

    let max_checkpoint = *tx_checkpoints.iter().max().unwrap();

    // Wait for all pipelines to catch up via the same path GraphQL uses.
    harness.wait_for_watermark(max_checkpoint, 0).await?;

    // -- Transaction lookup --
    let transactions = harness
        .bigtable_client()
        .get_transactions(&tx_digests)
        .await?;
    assert_eq!(transactions.len(), signed_txns.len());
    for signed in &signed_txns {
        let indexed = transactions
            .iter()
            .find(|td| td.transaction.digest() == signed.digest())
            .unwrap_or_else(|| panic!("transaction {} not found in results", signed.digest()));
        assert_eq!(indexed.transaction, *signed);
        assert!(indexed.checkpoint_number > 0);
        assert!(indexed.timestamp > 0);
    }

    // -- Checkpoint lookup --
    let checkpoint_numbers: Vec<_> = transactions
        .iter()
        .map(|td| td.checkpoint_number)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let checkpoints = harness
        .bigtable_client()
        .get_checkpoints(&checkpoint_numbers)
        .await?;
    assert_eq!(checkpoints.len(), checkpoint_numbers.len());
    for cp in &checkpoints {
        assert!(checkpoint_numbers.contains(&cp.summary.sequence_number));
        assert!(cp.summary.epoch == 0);

        let content_digests: Vec<_> = cp.contents.iter().map(|ed| ed.transaction).collect();
        let expected: Vec<_> = tx_digests
            .iter()
            .zip(&tx_checkpoints)
            .filter(|(_, cp_num)| **cp_num == cp.summary.sequence_number)
            .map(|(d, _)| *d)
            .collect();
        for d in &expected {
            assert!(
                content_digests.contains(d),
                "checkpoint {} should contain txn {}",
                cp.summary.sequence_number,
                d,
            );
        }
    }

    // -- Checkpoint-by-digest reverse index --
    for cp in &checkpoints {
        let digest = cp.summary.digest();
        let found = harness
            .bigtable_client()
            .get_checkpoint_by_digest(digest)
            .await?;
        assert!(found.is_some(), "checkpoint by digest should exist");
        assert_eq!(
            found.unwrap().summary.sequence_number,
            cp.summary.sequence_number
        );
    }

    // -- Objects lookup --
    let mut object_keys: Vec<ObjectKey> = Vec::new();
    for tx_data in &transactions {
        for (obj_ref, _owner, _write_kind) in tx_data.effects.all_changed_objects() {
            object_keys.push(ObjectKey(obj_ref.0, obj_ref.1));
        }
    }
    assert!(!object_keys.is_empty());
    let objects = harness.bigtable_client().get_objects(&object_keys).await?;
    assert_eq!(objects.len(), object_keys.len());
    for obj in &objects {
        assert!(
            object_keys
                .iter()
                .any(|k| k.0 == obj.id() && k.1 == obj.version()),
            "unexpected object {}v{}",
            obj.id(),
            obj.version().value(),
        );
    }

    // -- Epoch 0 before reconfig: start fields set, end fields not yet --
    let e0 = harness
        .bigtable_client()
        .get_epoch(0)
        .await?
        .expect("epoch 0");
    assert_eq!(e0.epoch, Some(0));
    assert!(e0.start_checkpoint.is_some());
    assert!(e0.start_timestamp_ms.is_some());
    assert!(e0.reference_gas_price.is_some());
    assert!(e0.end_checkpoint.is_none());
    assert!(e0.end_timestamp_ms.is_none());

    // -- Trigger epoch change --
    harness.cluster.trigger_reconfiguration().await;
    harness.wait_for_watermark(0, 1).await?;

    // Epoch 0 now has end fields populated
    let e0 = harness
        .bigtable_client()
        .get_epoch(0)
        .await?
        .expect("epoch 0");
    assert!(e0.end_checkpoint.is_some());
    assert!(e0.end_timestamp_ms.is_some());

    // Epoch 1 exists with start fields
    let e1 = harness
        .bigtable_client()
        .get_epoch(1)
        .await?
        .expect("epoch 1");
    assert_eq!(e1.epoch, Some(1));
    assert!(e1.start_checkpoint.is_some());
    assert!(e1.start_timestamp_ms.is_some());

    let latest = harness.bigtable_client().get_latest_epoch().await?;
    assert!(latest.is_some());
    assert!(latest.unwrap().epoch.unwrap() >= 1);

    Ok(())
}
