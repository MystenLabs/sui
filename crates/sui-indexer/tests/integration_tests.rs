// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// integration test with standalone postgresql database
#[cfg(feature = "pg_integration")]
mod pg_integration {
    use diesel::migration::MigrationSource;
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
    use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
    use move_core_types::ident_str;
    use move_core_types::identifier::Identifier;
    use move_core_types::language_storage::StructTag;
    use prometheus::Registry;
    use std::env;
    use std::str::FromStr;
    use sui_config::SUI_KEYSTORE_FILENAME;
    use sui_indexer::errors::IndexerError;
    use sui_indexer::store::{IndexerStore, PgIndexerStore};
    use sui_indexer::{new_pg_connection_pool, Indexer, IndexerConfig, PgPoolConnection};
    use sui_json_rpc::api::EventReadApiClient;
    use sui_json_rpc::api::{ReadApiClient, TransactionBuilderClient, WriteApiClient};
    use sui_json_rpc_types::{
        EventFilter, SuiMoveObject, SuiObjectDataOptions, SuiObjectResponse,
        SuiObjectResponseQuery, SuiParsedMoveObject, SuiTransactionResponseOptions,
        SuiTransactionResponseQuery, TransactionBytes,
    };
    use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
    use sui_types::base_types::ObjectID;
    use sui_types::digests::TransactionDigest;
    use sui_types::gas_coin::GasCoin;
    use sui_types::messages::ExecuteTransactionRequestType;
    use sui_types::object::ObjectFormatOptions;
    use sui_types::query::TransactionFilter;
    use sui_types::utils::to_sender_signed_transaction;
    use sui_types::SUI_FRAMEWORK_ADDRESS;
    use test_utils::network::{TestCluster, TestClusterBuilder};
    use test_utils::transaction::{create_devnet_nft, delete_devnet_nft, transfer_coin};

    use tokio::task::JoinHandle;
    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

    #[tokio::test]
    async fn test_genesis_sync() {
        let (test_cluster, indexer_rpc_client, store, handle) = start_test_cluster().await;
        // Allow indexer to sync
        wait_until_next_checkpoint(&store).await;

        let checkpoint = store.get_checkpoint(0.into()).unwrap();

        for tx in checkpoint.transactions {
            let tx = tx.unwrap();
            let transaction = store.get_transaction_by_digest(&tx);
            assert!(transaction.is_ok());
            let tx_digest = TransactionDigest::from_str(&tx).unwrap();
            let _fullnode_rpc_tx = test_cluster
                .rpc_client()
                .get_transaction_with_options(tx_digest, Some(SuiTransactionResponseOptions::new()))
                .await
                .unwrap();
            let _indexer_rpc_tx = indexer_rpc_client
                .get_transaction_with_options(tx_digest, Some(SuiTransactionResponseOptions::new()))
                .await
                .unwrap();

            // This fails because of events mismatch
            // TODO: fix this
            //assert_eq!(fullnode_rpc_tx, indexer_rpc_tx);
        }
        // TODO: more checks to ensure genesis sync data integrity.
        drop(handle);
    }

    #[tokio::test]
    async fn test_simple_transaction_e2e() -> Result<(), anyhow::Error> {
        let (test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster().await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;
        let address = test_cluster.accounts.first().unwrap();
        let recipient_address = test_cluster.accounts.last().unwrap();
        let gas_objects: Vec<ObjectID> = indexer_rpc_client
            .get_owned_objects(
                *address,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::new().with_type(),
                )),
                None,
                None,
                None,
            )
            .await?
            .data
            .into_iter()
            .filter_map(|object_resp| match object_resp {
                SuiObjectResponse::Exists(obj_data) => Some(obj_data.object_id),
                _ => None,
            })
            .collect();

        let transaction_bytes: TransactionBytes = indexer_rpc_client
            .transfer_object(
                *address,
                *gas_objects.first().unwrap(),
                Some(*gas_objects.last().unwrap()),
                2000,
                *recipient_address,
            )
            .await?;

        let keystore_path = test_cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
        let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
        let tx =
            to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(address)?);
        let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

        let tx_response = indexer_rpc_client
            .execute_transaction(
                tx_bytes,
                signatures,
                Some(SuiTransactionResponseOptions::full_content()),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;

        wait_until_transaction_synced(&store, tx_response.digest.base58_encode().as_str()).await;
        let tx_read_response = indexer_rpc_client
            .get_transaction_with_options(
                tx_response.digest,
                Some(SuiTransactionResponseOptions::full_content()),
            )
            .await?;
        assert_eq!(tx_response.digest, tx_read_response.digest);
        assert_eq!(tx_response.transaction, tx_read_response.transaction);
        assert_eq!(tx_response.effects, tx_read_response.effects);

        // query txn with sender address
        let from_query =
            SuiTransactionResponseQuery::new_with_filter(TransactionFilter::FromAddress(*address));
        let tx_from_query_response = indexer_rpc_client
            .query_transactions(from_query, None, None, None)
            .await?;
        assert!(!tx_from_query_response.has_next_page);
        assert_eq!(tx_from_query_response.data.len(), 1);
        assert_eq!(
            tx_response.digest,
            tx_from_query_response.data.first().unwrap().digest
        );

        // query txn with recipient address
        let to_query = SuiTransactionResponseQuery::new_with_filter(TransactionFilter::ToAddress(
            *recipient_address,
        ));
        let tx_to_query_response = indexer_rpc_client
            .query_transactions(to_query, None, None, None)
            .await?;
        // the address has received 2 transactions, one is genesis
        assert!(!tx_to_query_response.has_next_page);
        assert_eq!(tx_to_query_response.data.len(), 2);
        assert_eq!(
            tx_response.digest,
            tx_to_query_response.data.last().unwrap().digest
        );

        // query txn with mutated object id
        let mutation_query = SuiTransactionResponseQuery::new_with_filter(
            TransactionFilter::ChangedObject(*gas_objects.first().unwrap()),
        );
        let tx_mutation_query_response = indexer_rpc_client
            .query_transactions(mutation_query, None, None, None)
            .await?;

        // the coin is first created by genesis txn, then transferred by the above txn
        assert!(!tx_mutation_query_response.has_next_page);
        assert_eq!(tx_mutation_query_response.data.len(), 2);
        assert_eq!(
            tx_response.digest,
            tx_mutation_query_response.data.last().unwrap().digest,
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_event_query_e2e() -> Result<(), anyhow::Error> {
        let (mut test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster().await;
        wait_until_next_checkpoint(&store).await;
        let nft_creator = test_cluster.get_address_0();
        let context = &mut test_cluster.wallet;

        let (_, _, digest_one) = create_devnet_nft(context).await.unwrap();
        wait_until_transaction_synced(&store, digest_one.base58_encode().as_str()).await;
        let (_, _, digest_two) = create_devnet_nft(context).await.unwrap();
        wait_until_transaction_synced(&store, digest_two.base58_encode().as_str()).await;
        let (transferred_object, sender, receiver, digest_three, _, _) =
            transfer_coin(context).await.unwrap();
        wait_until_transaction_synced(&store, digest_three.base58_encode().as_str()).await;

        // Test various ways of querying events
        let filter_on_sender = EventFilter::Sender(sender);
        let query_response = indexer_rpc_client
            .query_events(filter_on_sender, None, None, None)
            .await?;

        assert_eq!(query_response.data.len(), 2);
        for item in query_response.data {
            assert_eq!(item.transaction_module, ident_str!("devnet_nft").into());
            assert_eq!(item.package_id, ObjectID::from(SUI_FRAMEWORK_ADDRESS));
            assert_eq!(item.sender, nft_creator);
            assert_eq!(
                item.type_,
                StructTag::from_str("0x2::devnet_nft::MintNFTEvent").unwrap()
            );
        }

        let filter_on_transaction = EventFilter::Transaction(digest_one);
        let query_response = indexer_rpc_client
            .query_events(filter_on_transaction, None, None, None)
            .await?;
        assert_eq!(query_response.data.len(), 1);
        assert_eq!(
            digest_one,
            query_response.data.first().unwrap().id.tx_digest
        );

        let filter_on_module = EventFilter::MoveModule {
            package: ObjectID::from(SUI_FRAMEWORK_ADDRESS),
            module: Identifier::new("devnet_nft").unwrap(),
        };
        let query_response = indexer_rpc_client
            .query_events(filter_on_module, None, None, None)
            .await?;
        assert_eq!(query_response.data.len(), 2);
        assert_eq!(digest_one, query_response.data[0].id.tx_digest);
        assert_eq!(digest_two, query_response.data[1].id.tx_digest);

        let filter_on_event_type = EventFilter::MoveEventType(
            StructTag::from_str("0x2::devnet_nft::MintNFTEvent").unwrap(),
        );
        let query_response = indexer_rpc_client
            .query_events(filter_on_event_type, None, None, None)
            .await?;
        assert_eq!(query_response.data.len(), 2);
        assert_eq!(digest_one, query_response.data[0].id.tx_digest);
        assert_eq!(digest_two, query_response.data[1].id.tx_digest);

        // Verify that the transfer coin event occurred successfully, without emitting an event
        let object_correctly_transferred = indexer_rpc_client
            .get_owned_objects(
                receiver,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::full_content(),
                )),
                None,
                None,
                None,
            )
            .await?
            .data
            .into_iter()
            .filter_map(|object_resp| match object_resp {
                SuiObjectResponse::Exists(obj_data) => Some(obj_data),
                _ => None,
            })
            .any(|obj| obj.object_id == transferred_object);
        assert!(object_correctly_transferred);

        Ok(())
    }

    #[tokio::test]
    async fn test_event_query_pagination_e2e() -> Result<(), anyhow::Error> {
        let (mut test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster().await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;
        let context = &mut test_cluster.wallet;

        for _ in 0..5 {
            let (sender, object_id, digest) = create_devnet_nft(context).await.unwrap();
            wait_until_transaction_synced(&store, digest.base58_encode().as_str()).await;
            let obj_resp = indexer_rpc_client
                .get_object_with_options(object_id, None)
                .await
                .unwrap();
            let data = obj_resp.object()?;
            let result = delete_devnet_nft(
                context,
                &sender,
                (data.object_id, data.version, data.digest),
            )
            .await;
            wait_until_transaction_synced(&store, result.digest.base58_encode().as_str()).await;
        }

        let filter_on_module = EventFilter::MoveModule {
            package: ObjectID::from(SUI_FRAMEWORK_ADDRESS),
            module: Identifier::new("devnet_nft").unwrap(),
        };
        let query_response = indexer_rpc_client
            .query_events(filter_on_module, None, None, None)
            .await?;
        assert_eq!(query_response.data.len(), 5);

        let mint_nft_event = "0x2::devnet_nft::MintNFTEvent";
        let filter = get_filter_on_event_type(mint_nft_event);
        let query_response = indexer_rpc_client
            .query_events(filter, None, Some(2), None)
            .await?;
        assert!(query_response.has_next_page);
        assert_eq!(query_response.data.len(), 2);

        let filter = get_filter_on_event_type(mint_nft_event);
        let cursor = query_response.next_cursor;
        let query_response = indexer_rpc_client
            .query_events(filter, cursor, Some(4), None)
            .await?;
        assert!(!query_response.has_next_page);
        assert_eq!(query_response.data.len(), 3);

        // This move module does not explicitly emit an event
        let burn_nft_event = "0x2::devnet_nft::BurnNFTEvent";
        let filter = get_filter_on_event_type(burn_nft_event);
        let query_response = indexer_rpc_client
            .query_events(filter, None, Some(4), None)
            .await?;
        assert!(!query_response.has_next_page);
        assert_eq!(query_response.data.len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_module_cache() {
        let (test_cluster, _, store, handle) = start_test_cluster().await;
        let coins = test_cluster
            .sui_client()
            .coin_read_api()
            .get_coins(test_cluster.get_address_0(), None, None, None)
            .await
            .unwrap()
            .data;
        // Allow indexer to sync
        wait_until_next_checkpoint(&store).await;

        let coin_object = store
            .get_object(coins[0].coin_object_id, Some(coins[0].version))
            .unwrap()
            .into_object()
            .unwrap();

        let layout = coin_object
            .get_layout(ObjectFormatOptions::default(), store.module_cache())
            .unwrap();

        assert!(layout.is_some());

        let layout = layout.unwrap();

        let parsed_coin = SuiParsedMoveObject::try_from_layout(
            coin_object.data.try_as_move().unwrap().clone(),
            layout,
        )
        .unwrap();

        assert_eq!(GasCoin::type_(), parsed_coin.type_);
        drop(handle);
    }

    async fn start_test_cluster() -> (
        TestCluster,
        HttpClient,
        PgIndexerStore,
        JoinHandle<Result<(), IndexerError>>,
    ) {
        let pg_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into());
        let pg_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "32771".into());
        let pw = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgrespw".into());
        let db_url = format!("postgres://postgres:{pw}@{pg_host}:{pg_port}");
        let pg_connection_pool = new_pg_connection_pool(&db_url).await.unwrap();

        reset_database(&mut pg_connection_pool.get().unwrap());

        let test_cluster = TestClusterBuilder::new().build().await.unwrap();
        let store = PgIndexerStore::new(pg_connection_pool);

        let store_clone = store.clone();
        let registry = Registry::default();

        let mut config = IndexerConfig::default();
        config.rpc_client_url = test_cluster.rpc_url().to_string();
        let indexer_config = config.clone();
        let handle =
            tokio::spawn(
                async move { Indexer::start(&indexer_config, &registry, store_clone).await },
            );

        let http_addr_port = format!(
            "http://{}:{}",
            config.rpc_server_url, config.rpc_server_port
        );
        let http_client = HttpClientBuilder::default().build(http_addr_port).unwrap();

        (test_cluster, http_client, store, handle)
    }

    async fn wait_until_next_checkpoint(store: &PgIndexerStore) {
        let mut cp = store.get_latest_checkpoint_sequence_number().unwrap();
        let target = cp + 1;
        while cp < target {
            tokio::task::yield_now().await;
            cp = store.get_latest_checkpoint_sequence_number().unwrap();
        }
    }

    async fn wait_until_transaction_synced(store: &PgIndexerStore, tx_digest: &str) {
        let mut tx = store.get_transaction_by_digest(tx_digest);
        while tx.is_err() {
            tokio::task::yield_now().await;
            tx = store.get_transaction_by_digest(tx_digest);
        }
    }

    fn get_filter_on_event_type(event_type: &str) -> EventFilter {
        EventFilter::MoveEventType(StructTag::from_str(event_type).unwrap())
    }

    fn reset_database(conn: &mut PgPoolConnection) {
        conn.revert_all_migrations(MIGRATIONS).unwrap();
        conn.run_migrations(&MIGRATIONS.migrations().unwrap())
            .unwrap();
    }
}
