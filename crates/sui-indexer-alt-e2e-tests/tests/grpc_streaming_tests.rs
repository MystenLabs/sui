// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use diesel::QueryDsl;
use diesel_async::RunQueryDsl;
use sui_field_count::FieldCount;
use sui_indexer_alt_framework::{
    cluster::IndexerCluster,
    ingestion::ClientArgs,
    pipeline::{
        concurrent::{self, ConcurrentConfig},
        Processor,
    },
};
use sui_pg_db::{Db, DbArgs};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::full_checkpoint_content::CheckpointData;
use tempfile::tempdir;
use test_cluster::TestClusterBuilder;

diesel::table! {
    /// Table for storing transaction counts per checkpoint.
    tx_counts (cp_sequence_number) {
    cp_sequence_number -> BigInt,
    count -> BigInt,
    }
}

#[derive(diesel::Queryable, diesel::Insertable, diesel::Selectable, Clone, Debug, FieldCount)]
#[diesel(table_name = tx_counts)]
struct StoredTxCount {
    cp_sequence_number: i64,
    count: i64,
}

/// Test concurrent pipeline for populating [tx_counts].
struct TxCounts;

impl Processor for TxCounts {
    const NAME: &'static str = "tx_counts";
    type Value = StoredTxCount;

    fn process(
        &self,
        checkpoint: &std::sync::Arc<CheckpointData>,
    ) -> anyhow::Result<Vec<Self::Value>> {
        Ok(vec![StoredTxCount {
            cp_sequence_number: checkpoint.checkpoint_summary.sequence_number as i64,
            count: checkpoint.transactions.len() as i64,
        }])
    }
}

#[async_trait::async_trait]
impl concurrent::Handler for TxCounts {
    type Store = Db;

    async fn commit<'a>(
        values: &[Self::Value],
        conn: &mut sui_pg_db::Connection<'a>,
    ) -> anyhow::Result<usize> {
        Ok(diesel::insert_into(tx_counts::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}

/// Helper function to transfer a coin in the test cluster
async fn transfer_coin(
    wallet: &mut sui_sdk::wallet_context::WalletContext,
) -> sui_types::base_types::TransactionDigest {
    let sender = wallet.active_address().unwrap();
    let gas_objs = wallet
        .gas_for_owner_budget(sender, 5000, Default::default())
        .await
        .unwrap();
    let gas_obj = gas_objs.1.object_ref();

    let tx_data = TestTransactionBuilder::new(sender, gas_obj, 1000)
        .transfer_sui(Some(1000), sender)
        .build();
    let tx = wallet.sign_transaction(&tx_data).await;

    wallet.execute_transaction_must_succeed(tx).await.digest
}

#[tokio::test]
async fn test_indexer_cluster_with_grpc_streaming() {
    use sui_pg_db::temp::TempDb;

    // Create a directory for local ingestion as fallback
    let checkpoint_dir: tempfile::TempDir = tempdir().unwrap();

    // Start a TestCluster with a fullnode that provides gRPC streaming
    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_data_ingestion_dir(checkpoint_dir.path().to_owned())
        .build()
        .await;

    // Create a temporary database for the indexer
    let db = TempDb::new().expect("Failed to create temporary database");
    let url = db.database().url();

    // Generate some test transactions to create checkpoints
    let wallet = &mut test_cluster.wallet;
    for _ in 0..5 {
        transfer_coin(wallet).await;
    }

    // Set up the indexer with gRPC streaming endpoint from TestCluster
    let client_args = ClientArgs {
        local_ingestion_path: Some(checkpoint_dir.path().to_owned()),
        remote_store_url: None,
        rpc_api_url: None,
        rpc_username: None,
        rpc_password: None,
        // Use the TestCluster's RPC URL as the gRPC streaming endpoint
        streaming_endpoint: Some(test_cluster.rpc_url().to_string()),
    };

    // Create writer/reader for database operations
    let reader = Db::for_read(url.clone(), DbArgs::default()).await.unwrap();
    let writer = Db::for_write(url.clone(), DbArgs::default()).await.unwrap();

    // Create the tx_counts table
    {
        let mut conn = writer.connect().await.unwrap();
        diesel::sql_query(
            r#"
            CREATE TABLE tx_counts (
                cp_sequence_number  BIGINT PRIMARY KEY,
                count               BIGINT NOT NULL
            )
            "#,
        )
        .execute(&mut conn)
        .await
        .unwrap();
    }

    // Build the indexer cluster with gRPC streaming configuration
    let mut indexer = IndexerCluster::builder()
        .with_database_url(url.clone())
        .with_client_args(client_args)
        .build()
        .await
        .unwrap();

    // Add the tx_counts pipeline
    indexer
        .concurrent_pipeline(TxCounts, ConcurrentConfig::default())
        .await
        .unwrap();

    let metrics = indexer.metrics().clone();
    let cancel = indexer.cancel().clone();

    // Run the indexer - it will use gRPC streaming from TestCluster
    // and fall back to local ingestion if needed
    let handle = indexer.run().await.unwrap();

    tokio::time::sleep(Duration::from_secs(5)).await;

    // Check that results were written
    {
        let mut conn = reader.connect().await.unwrap();
        let counts: Vec<StoredTxCount> = tx_counts::table
            .order_by(tx_counts::cp_sequence_number)
            .load(&mut conn)
            .await
            .unwrap();

        println!("Tx counts: {:?}", counts);
        // We should have processed some checkpoints.
        assert!(!counts.is_empty());
        for (i, count) in counts.iter().enumerate() {
            assert_eq!(count.cp_sequence_number, i as i64);
            // Each checkpoint should have at least one transaction
            assert!(count.count > 0);
        }
    }

    // Verify metrics show that checkpoints were ingested
    assert!(metrics.total_ingested_checkpoints.get() >= 5);

    // Verify pipeline metrics
    assert!(
        metrics
            .total_handler_checkpoints_received
            .get_metric_with_label_values(&["tx_counts"])
            .unwrap()
            .get()
            >= 5
    );

    println!("here");

    cancel.cancel();
    println!("here here");
    handle.await.unwrap();
}
