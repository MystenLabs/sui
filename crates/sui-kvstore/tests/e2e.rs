// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end integration tests for the BigTable KV Store.
//!
//! Each test spawns its own BigTable emulator process on a random port,
//! creates the required tables, and tears everything down when done.
//! Tests are skipped automatically if `gcloud` is not on PATH.

use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_indexer_alt_framework::ingestion::streaming_client::StreamingClientArgs;
use sui_indexer_alt_framework::ingestion::{ClientArgs, IngestionConfig};
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_kvstore::{
    BigTableClient, BigTableIndexer, BigTableStore, CheckpointsPipeline, KeyValueStoreReader,
    TransactionsPipeline,
};
use sui_test_transaction_builder::make_transfer_sui_transaction;
use sui_types::base_types::SuiAddress;
use sui_types::gas_coin::MIST_PER_SUI;
use sui_types::transaction::Transaction;
use test_cluster::{TestCluster, TestClusterBuilder};
use tokio::time::interval;
use url::Url;

/// Pipeline names for watermark checking.
const CHECKPOINTS_PIPELINE: &str = <CheckpointsPipeline as Processor>::NAME;
const TRANSACTIONS_PIPELINE: &str = <TransactionsPipeline as Processor>::NAME;

const INSTANCE_ID: &str = "bigtable_test_instance";

const TABLES: &[&str] = &[
    sui_kvstore::tables::objects::NAME,
    sui_kvstore::tables::object_types::NAME,
    sui_kvstore::tables::transactions::NAME,
    sui_kvstore::tables::checkpoints::NAME,
    sui_kvstore::tables::checkpoints_by_digest::NAME,
    sui_kvstore::tables::watermark_legacy::NAME,
    sui_kvstore::tables::watermark_alt_legacy::NAME,
    sui_kvstore::tables::epochs::NAME,
    sui_kvstore::tables::watermarks::NAME,
];

fn require_bigtable_emulator() {
    assert!(
        Command::new("gcloud").arg("--version").output().is_ok(),
        "gcloud not found on PATH — install the Google Cloud SDK to run these tests"
    );
    assert!(
        Command::new("cbt").arg("-version").output().is_ok(),
        "cbt not found on PATH — run: gcloud components install cbt"
    );
    assert!(
        Command::new("gcloud")
            .args(["beta", "emulators", "bigtable", "env-init"])
            .output()
            .is_ok_and(|o| o.status.success()),
        "BigTable emulator not installed — run: gcloud components install bigtable"
    );
}

/// A self-contained BigTable emulator process.
/// Spawns the emulator on a random port and creates all required tables.
/// The emulator process is killed when this struct is dropped.
struct BigTableEmulator {
    child: Child,
    host: String,
    // Keep stderr open so the emulator doesn't get SIGPIPE and die.
    _stderr_drain: std::thread::JoinHandle<()>,
}

impl BigTableEmulator {
    fn start(instance_id: &str) -> Result<Self> {
        let mut child = Command::new("gcloud")
            .args([
                "beta",
                "emulators",
                "bigtable",
                "start",
                "--host-port=0.0.0.0:0",
            ])
            .stderr(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .context("Failed to spawn BigTable emulator")?;

        let stderr = child.stderr.take().expect("stderr was piped");
        let mut reader = std::io::BufReader::new(stderr);

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
            // The emulator prints: [bigtable] Cloud Bigtable emulator running on [::]:PORT
            if line_buf.contains("Cloud Bigtable emulator running on") {
                if let Some(addr) = line_buf.rsplit("running on ").next() {
                    let addr = addr.trim();
                    // The emulator binds to [::]:PORT; connect via localhost instead.
                    if let Some(port) = addr.rsplit(':').next() {
                        host = Some(format!("localhost:{port}"));
                    }
                }
                break;
            }
        }

        let host = host.context("Failed to parse emulator host:port from stderr")?;

        // Drain remaining stderr in a background thread to prevent SIGPIPE.
        let stderr_drain = std::thread::spawn(move || {
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
        });

        // Create tables using cbt
        for table in TABLES {
            let status = Command::new("cbt")
                .args(["-instance", instance_id, "-project", "emulator"])
                .arg("createtable")
                .arg(table)
                .env("BIGTABLE_EMULATOR_HOST", &host)
                .output()
                .with_context(|| format!("Failed to run cbt createtable {table}"))?;
            if !status.status.success() {
                bail!(
                    "cbt createtable {table} failed: {}",
                    String::from_utf8_lossy(&status.stderr)
                );
            }

            let status = Command::new("cbt")
                .args(["-instance", instance_id, "-project", "emulator"])
                .args(["createfamily", table, "sui"])
                .env("BIGTABLE_EMULATOR_HOST", &host)
                .output()
                .with_context(|| format!("Failed to run cbt createfamily {table}"))?;
            if !status.status.success() {
                bail!(
                    "cbt createfamily {table} failed: {}",
                    String::from_utf8_lossy(&status.stderr)
                );
            }
        }

        Ok(Self {
            child,
            host,
            _stderr_drain: stderr_drain,
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

/// Test cluster that combines a real TestCluster (validators + fullnode) with
/// a BigTable emulator and indexer for e2e testing.
struct BigTableCluster {
    cluster: TestCluster,
    client: BigTableClient,
    _emulator: BigTableEmulator,
    pipelines: Vec<&'static str>,
}

impl BigTableCluster {
    async fn new() -> Result<Self> {
        require_bigtable_emulator();

        let emulator =
            BigTableEmulator::start(INSTANCE_ID).context("Failed to start BigTable emulator")?;

        let client =
            BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string())
                .await
                .context("Failed to create BigTable client")?;

        let cluster = TestClusterBuilder::new().build().await;

        let store = BigTableStore::new(client.clone());
        let registry = prometheus::Registry::new();

        let indexer_args = IndexerArgs::default();
        let rpc_url = cluster.rpc_url();
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

        let pipelines = bigtable_indexer.pipeline_names();

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
            _emulator: emulator,
            pipelines,
        })
    }

    /// Execute a pre-signed transaction and wait until it appears in BigTable.
    ///
    /// Polls BigTable for the transaction digest rather than relying on watermark
    /// advancement, because TestCluster can produce empty checkpoints via consensus
    /// that advance watermarks before the transaction's checkpoint is indexed.
    async fn execute_transaction(&mut self, tx: Transaction) -> Result<()> {
        let digest = *tx.digest();
        self.cluster.execute_transaction(tx).await;
        tokio::time::timeout(Duration::from_secs(60), async {
            let mut interval = interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                if let Ok(txns) = self.client.get_transactions(&[digest]).await
                    && !txns.is_empty()
                {
                    break;
                }
            }
        })
        .await
        .context("Timeout waiting for transaction to be indexed in BigTable")?;
        Ok(())
    }

    /// Wait for all pipeline watermarks to exist and reach at least `checkpoint`.
    async fn wait_for_watermark(&mut self, checkpoint: u64) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(60), async {
            let mut interval = interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                if let Some(min) = self.get_min_watermark().await
                    && min >= checkpoint
                {
                    break;
                }
            }
        })
        .await
        .context("Timeout waiting for watermarks to advance")
    }

    async fn get_min_watermark(&mut self) -> Option<u64> {
        let mut min = None;
        for pipeline in &self.pipelines {
            if let Ok(Some(watermark)) = self.client.get_pipeline_watermark(pipeline).await {
                let checkpoint = watermark.checkpoint_hi_inclusive;
                min = Some(min.map_or(checkpoint, |m: u64| m.min(checkpoint)));
            } else {
                return None;
            }
        }
        min
    }

    fn bigtable_client(&mut self) -> &mut BigTableClient {
        &mut self.client
    }
}

#[tokio::test]
async fn test_transfer_transaction_indexed() -> Result<()> {
    let mut cluster = BigTableCluster::new().await?;

    let recipient = SuiAddress::random_for_testing_only();
    let tx =
        make_transfer_sui_transaction(&cluster.cluster.wallet, Some(recipient), Some(MIST_PER_SUI))
            .await;
    let tx_digest = *tx.digest();
    cluster.execute_transaction(tx).await?;

    let transactions = cluster
        .bigtable_client()
        .get_transactions(&[tx_digest])
        .await?;
    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].transaction.digest(), &tx_digest);

    Ok(())
}

#[tokio::test]
async fn test_checkpoint_indexed() -> Result<()> {
    let mut cluster = BigTableCluster::new().await?;

    let tx = make_transfer_sui_transaction(&cluster.cluster.wallet, None, None).await;
    cluster.execute_transaction(tx).await?;

    // Watermark may lag behind the transaction appearing in BigTable; wait for it.
    cluster.wait_for_watermark(1).await?;
    let watermark = cluster.get_min_watermark().await.expect("watermark exists");

    let checkpoints = cluster
        .bigtable_client()
        .get_checkpoints(&[watermark])
        .await?;
    assert_eq!(checkpoints.len(), 1);
    assert_eq!(checkpoints[0].summary.sequence_number, watermark);

    Ok(())
}

#[tokio::test]
async fn test_multiple_transactions_indexed() -> Result<()> {
    let mut cluster = BigTableCluster::new().await?;

    let mut tx_digests = Vec::new();
    for _ in 0..3 {
        let recipient = SuiAddress::random_for_testing_only();
        let tx = make_transfer_sui_transaction(
            &cluster.cluster.wallet,
            Some(recipient),
            Some(MIST_PER_SUI),
        )
        .await;
        tx_digests.push(*tx.digest());
        cluster.execute_transaction(tx).await?;
    }

    let transactions = cluster
        .bigtable_client()
        .get_transactions(&tx_digests)
        .await?;
    assert_eq!(transactions.len(), 3);

    let returned_digests: std::collections::HashSet<_> = transactions
        .iter()
        .map(|tx| tx.transaction.digest())
        .collect();
    for expected_digest in &tx_digests {
        assert!(
            returned_digests.contains(expected_digest),
            "Expected transaction {} not found in results",
            expected_digest
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_watermarks_updated() -> Result<()> {
    let mut cluster = BigTableCluster::new().await?;

    let tx = make_transfer_sui_transaction(&cluster.cluster.wallet, None, None).await;
    cluster.execute_transaction(tx).await?;
    cluster.wait_for_watermark(1).await?;

    let checkpoints_watermark = cluster
        .bigtable_client()
        .get_pipeline_watermark(CHECKPOINTS_PIPELINE)
        .await?;
    let transactions_watermark = cluster
        .bigtable_client()
        .get_pipeline_watermark(TRANSACTIONS_PIPELINE)
        .await?;

    assert!(checkpoints_watermark.is_some());
    assert!(transactions_watermark.is_some());

    let checkpoints_wm = checkpoints_watermark.unwrap();
    let transactions_wm = transactions_watermark.unwrap();

    assert!(checkpoints_wm.checkpoint_hi_inclusive > 0);
    assert!(transactions_wm.checkpoint_hi_inclusive > 0);

    Ok(())
}
