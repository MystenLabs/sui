// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "pg_integration")]
mod ingestion_tests {
    use diesel::ExpressionMethods;
    use diesel::{QueryDsl, RunQueryDsl};
    use move_core_types::language_storage::StructTag;
    use simulacrum::Simulacrum;
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;
    use sui_indexer::db::get_pool_connection;
    use sui_indexer::errors::Context;
    use sui_indexer::errors::IndexerError;
    use sui_indexer::models::{
        events::StoredEvent, objects::StoredObject, transactions::StoredTransaction,
    };
    use sui_indexer::schema::{events, objects, transactions};
    use sui_indexer::store::{indexer_store::IndexerStore, PgIndexerStore};
    use sui_indexer::test_utils::{start_test_indexer, ReaderWriterConfig};
    use sui_types::base_types::SuiAddress;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::gas_coin::GasCoin;
    use sui_types::{
        Identifier, SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_ADDRESS, SUI_SYSTEM_PACKAGE_ID,
    };
    use tempfile::tempdir;
    use tokio::task::JoinHandle;

    macro_rules! read_only_blocking {
        ($pool:expr, $query:expr) => {{
            let mut pg_pool_conn = get_pool_connection::<diesel::PgConnection>($pool)?;
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
        data_ingestion_path: PathBuf,
    ) -> (
        JoinHandle<()>,
        PgIndexerStore<diesel::PgConnection>,
        JoinHandle<Result<(), IndexerError>>,
    ) {
        let server_url: SocketAddr = format!("127.0.0.1:{}", DEFAULT_SERVER_PORT)
            .parse()
            .unwrap();

        let server_handle = tokio::spawn(async move {
            sui_rest_api::RestService::new_without_version(sim)
                .start_service(server_url, Some("/rest".to_owned()))
                .await;
        });
        // Starts indexer
        let (pg_store, pg_handle) = start_test_indexer(
            Some(DEFAULT_DB_URL.to_owned()),
            format!("http://{}", server_url),
            ReaderWriterConfig::writer_mode(None),
            data_ingestion_path,
        )
        .await;
        (server_handle, pg_store, pg_handle)
    }

    /// Wait for the indexer to catch up to the given checkpoint sequence number.
    async fn wait_for_checkpoint(
        pg_store: &PgIndexerStore<diesel::PgConnection>,
        checkpoint_sequence_number: u64,
    ) -> Result<(), IndexerError> {
        tokio::time::timeout(Duration::from_secs(10), async {
            while {
                let cp_opt = pg_store
                    .get_latest_checkpoint_sequence_number()
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

    /// Wait for the indexer to catch up to the given epoch id.
    async fn wait_for_epoch(
        pg_store: &PgIndexerStore<diesel::PgConnection>,
        epoch: u64,
    ) -> Result<(), IndexerError> {
        tokio::time::timeout(Duration::from_secs(10), async {
            while {
                let cp_opt = pg_store.get_latest_epoch_id().unwrap();
                cp_opt.is_none() || (cp_opt.unwrap() < epoch)
            } {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
        .await
        .expect("Timeout waiting for indexer to catchup to epoch");
        Ok(())
    }

    #[tokio::test]
    pub async fn test_transaction_table() -> Result<(), IndexerError> {
        let mut sim = Simulacrum::new();
        let data_ingestion_path = tempdir().unwrap().into_path();
        sim.set_data_ingestion_path(data_ingestion_path.clone());

        // Execute a simple transaction.
        let transfer_recipient = SuiAddress::random_for_testing_only();
        let (transaction, _) = sim.transfer_txn(transfer_recipient);
        let (effects, err) = sim.execute_transaction(transaction.clone()).unwrap();
        assert!(err.is_none());

        // Create a checkpoint which should include the transaction we executed.
        let checkpoint = sim.create_checkpoint();

        let (_, pg_store, _) = set_up(Arc::new(sim), data_ingestion_path).await;

        // Wait for the indexer to catch up to the checkpoint.
        wait_for_checkpoint(&pg_store, 1).await?;

        let digest = effects.transaction_digest();

        // Read the transaction from the database directly.
        let db_txn: StoredTransaction = read_only_blocking!(&pg_store.blocking_cp(), |conn| {
            transactions::table
                .filter(transactions::transaction_digest.eq(digest.inner().to_vec()))
                .first::<StoredTransaction>(conn)
        })
        .context("Failed reading transaction from PostgresDB")?;

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

    #[tokio::test]
    pub async fn test_event_type() -> Result<(), IndexerError> {
        let mut sim = Simulacrum::new();
        let data_ingestion_path = tempdir().unwrap().into_path();
        sim.set_data_ingestion_path(data_ingestion_path.clone());

        // Advance the epoch to generate some events.
        sim.advance_epoch(false);

        let (_, pg_store, _) = set_up(Arc::new(sim), data_ingestion_path).await;

        // Wait for the epoch to change so we can get some events.
        wait_for_epoch(&pg_store, 1).await?;

        // Read the event from the database directly.
        let db_event: StoredEvent = read_only_blocking!(&pg_store.blocking_cp(), |conn| {
            events::table
                .filter(events::event_type_name.eq("SystemEpochInfoEvent"))
                .first::<StoredEvent>(conn)
        })
        .context("Failed reading SystemEpochInfoEvent from PostgresDB")?;

        let event_type_tag = StructTag {
            address: SUI_SYSTEM_ADDRESS,
            module: Identifier::new("sui_system_state_inner").unwrap(),
            name: Identifier::new("SystemEpochInfoEvent").unwrap(),
            type_params: vec![],
        };

        // Check that the different components of the event type were stored correctly.
        assert_eq!(
            db_event.event_type,
            event_type_tag.to_canonical_string(true)
        );
        assert_eq!(db_event.event_type_package, SUI_SYSTEM_PACKAGE_ID.to_vec());
        assert_eq!(db_event.event_type_module, "sui_system_state_inner");
        assert_eq!(db_event.event_type_name, "SystemEpochInfoEvent");
        Ok(())
    }

    #[tokio::test]
    pub async fn test_object_type() -> Result<(), IndexerError> {
        let mut sim = Simulacrum::new();
        let data_ingestion_path = tempdir().unwrap().into_path();
        sim.set_data_ingestion_path(data_ingestion_path.clone());

        // Execute a simple transaction.
        let transfer_recipient = SuiAddress::random_for_testing_only();
        let (transaction, _) = sim.transfer_txn(transfer_recipient);
        let (_, err) = sim.execute_transaction(transaction.clone()).unwrap();
        assert!(err.is_none());

        // Create a checkpoint which should include the transaction we executed.
        let _ = sim.create_checkpoint();

        let (_, pg_store, _) = set_up(Arc::new(sim), data_ingestion_path).await;

        // Wait for the indexer to catch up to the checkpoint.
        wait_for_checkpoint(&pg_store, 1).await?;

        let obj_id = transaction.gas()[0].0;

        // Read the transaction from the database directly.
        let db_object: StoredObject = read_only_blocking!(&pg_store.blocking_cp(), |conn| {
            objects::table
                .filter(objects::object_id.eq(obj_id.to_vec()))
                .first::<StoredObject>(conn)
        })
        .context("Failed reading object from PostgresDB")?;

        let obj_type_tag = GasCoin::type_();

        // Check that the different components of the event type were stored correctly.
        assert_eq!(
            db_object.object_type,
            Some(obj_type_tag.to_canonical_string(true))
        );
        assert_eq!(
            db_object.object_type_package,
            Some(SUI_FRAMEWORK_PACKAGE_ID.to_vec())
        );
        assert_eq!(db_object.object_type_module, Some("coin".to_string()));
        assert_eq!(db_object.object_type_name, Some("Coin".to_string()));
        Ok(())
    }
}
