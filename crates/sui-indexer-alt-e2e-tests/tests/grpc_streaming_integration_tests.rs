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
        Processor,
        concurrent::{self, BatchStatus, ConcurrentConfig},
    },
};
use sui_pg_db::{Db, DbArgs};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{full_checkpoint_content::Checkpoint, transaction::TransactionDataAPI};
use tempfile::tempdir;
use test_cluster::TestClusterBuilder;

diesel::table! {
    /// Table for storing user transaction counts per checkpoint.
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

#[async_trait::async_trait]
impl Processor for TxCounts {
    const NAME: &'static str = "tx_counts";
    type Value = StoredTxCount;

    async fn process(
        &self,
        checkpoint: &std::sync::Arc<Checkpoint>,
    ) -> anyhow::Result<Vec<Self::Value>> {
        let user_tx_count = checkpoint
            .transactions
            .iter()
            .filter(|tx| !tx.transaction.is_system_tx())
            .count();

        Ok(vec![StoredTxCount {
            cp_sequence_number: checkpoint.summary.sequence_number as i64,
            count: user_tx_count as i64,
        }])
    }
}

#[async_trait::async_trait]
impl concurrent::Handler for TxCounts {
    type Store = Db;
    type Batch = Vec<Self::Value>;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        batch.extend(values);
        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        values: &Vec<Self::Value>,
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
        // Use the TestCluster's RPC URL as the gRPC streaming endpoint
        streaming_url: Some(test_cluster.rpc_url().parse().unwrap()),
        ..Default::default()
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

    let cancel = indexer.cancel().clone();

    // Run the indexer - it will use gRPC streaming from TestCluster
    // and fall back to local ingestion if needed
    let handle = indexer.run().await.unwrap();

    // Poll every 100ms with a 5s timeout for the sum of user transactions to reach 5
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        let mut num_txns = 0;
        while num_txns < 5 {
            interval.tick().await;

            let mut conn = reader.connect().await.unwrap();
            num_txns = tx_counts::table
                .select(tx_counts::count)
                .load::<i64>(&mut conn)
                .await
                .unwrap()
                .iter()
                .sum();
        }
    })
    .await
    .expect("Timeout: Expected sum of user transactions to reach 5 within 5 seconds");

    cancel.cancel();
    handle.await.unwrap();
}
