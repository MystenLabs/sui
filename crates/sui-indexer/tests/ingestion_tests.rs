// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "pg_integration")]
mod ingestion_tests {
    use diesel::ExpressionMethods;
    use diesel::{QueryDsl, RunQueryDsl};
    use simulacrum::Simulacrum;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Duration;
    use sui_indexer::db::get_pg_pool_connection;
    use sui_indexer::errors::Context;
    use sui_indexer::errors::IndexerError;
    use sui_indexer::models::transactions::StoredTransaction;
    use sui_indexer::schema::transactions;
    use sui_indexer::store::{indexer_store::IndexerStore, PgIndexerStore};
    use sui_indexer::test_utils::{start_test_indexer, ReaderWriterConfig};
    use sui_types::base_types::SuiAddress;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::storage::ReadStore;
    use tokio::task::JoinHandle;

    macro_rules! read_only_blocking {
        ($pool:expr, $query:expr) => {{
            let mut pg_pool_conn = get_pg_pool_connection($pool)?;
            pg_pool_conn
                .build_transaction()
                .read_only()
                .run($query)
                .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
        }};
    }

    const DEFAULT_SERVER_PORT: u16 = 3000;
    const DEFAULT_DB_URL: &str = "postgres://postgres:postgrespw@localhost:5432/sui_indexer";

    /// Set up a test indexer fetching from a REST endpoint served by the given Simulacrum.
    async fn set_up(
        sim: Arc<Simulacrum>,
    ) -> (
        JoinHandle<()>,
        PgIndexerStore,
        JoinHandle<Result<(), IndexerError>>,
    ) {
        let server_url: SocketAddr = format!("127.0.0.1:{}", DEFAULT_SERVER_PORT)
            .parse()
            .unwrap();

        let server_handle = tokio::spawn(async move {
            let chain_id = (*sim
                .get_checkpoint_by_sequence_number(0)
                .unwrap()
                .unwrap()
                .digest())
            .into();

            sui_rest_api::RestService::new_without_version(sim, chain_id)
                .start_service(server_url, Some("/rest".to_owned()))
                .await;
        });
        // Starts indexer
        let (pg_store, pg_handle) = start_test_indexer(
            Some(DEFAULT_DB_URL.to_owned()),
            format!("http://{}", server_url),
            ReaderWriterConfig::writer_mode(None),
        )
        .await;
        (server_handle, pg_store, pg_handle)
    }

    /// Wait for the indexer to catch up to the given checkpoint sequence number.
    async fn wait_for_checkpoint(
        pg_store: &PgIndexerStore,
        checkpoint_sequence_number: u64,
    ) -> Result<(), IndexerError> {
        tokio::time::timeout(Duration::from_secs(10), async {
            while {
                let cp_opt = pg_store
                    .get_latest_tx_checkpoint_sequence_number()
                    .await
                    .unwrap();
                cp_opt.is_none() || (cp_opt.unwrap() < checkpoint_sequence_number)
            } {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
        .await
        .expect("Timeout waiting for indexer to catchup to checkpoint");
        Ok(())
    }

    #[tokio::test]
    pub async fn test_transaction_table() -> Result<(), IndexerError> {
        let mut sim = Simulacrum::new();

        // Execute a simple transaction.
        let transfer_recipient = SuiAddress::random_for_testing_only();
        let (transaction, _) = sim.transfer_txn(transfer_recipient);
        let (effects, err) = sim.execute_transaction(transaction.clone()).unwrap();
        assert!(err.is_none());

        // Create a checkpoint which should include the transaction we executed.
        let checkpoint = sim.create_checkpoint();

        let (_, pg_store, _) = set_up(Arc::new(sim)).await;

        // Wait for the indexer to catch up to the checkpoint.
        wait_for_checkpoint(&pg_store, 1).await?;

        let digest = effects.transaction_digest();

        // Read the transaction from the database directly.
        let db_txn: StoredTransaction = read_only_blocking!(&pg_store.blocking_cp(), |conn| {
            transactions::table
                .filter(transactions::transaction_digest.eq(digest.inner().to_vec()))
                .first::<StoredTransaction>(conn)
        })
        .context("Failed reading latest checkpoint sequence number from PostgresDB")?;

        // Check that the transaction was stored correctly.
        assert_eq!(db_txn.tx_sequence_number, 1);
        assert_eq!(db_txn.transaction_digest, digest.inner().to_vec());
        assert_eq!(
            db_txn.raw_transaction,
            bcs::to_bytes(&transaction.data()).unwrap()
        );
        assert_eq!(db_txn.raw_effects, bcs::to_bytes(&effects).unwrap());
        assert_eq!(db_txn.timestamp_ms, checkpoint.timestamp_ms as i64);
        assert_eq!(db_txn.checkpoint_sequence_number, 1);
        assert_eq!(db_txn.transaction_kind, 1);
        assert_eq!(db_txn.success_command_count, 2); // split coin + transfer
        Ok(())
    }
}
