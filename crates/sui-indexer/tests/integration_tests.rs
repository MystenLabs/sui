// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// integration test with standalone postgresql database
#[cfg(feature = "pg_integration")]
pub mod pg_integration_test {
    use diesel::RunQueryDsl;
    use futures::future::join_all;
    use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
    use move_core_types::ident_str;
    use move_core_types::identifier::Identifier;
    use move_core_types::language_storage::StructTag;
    use move_core_types::parser::parse_struct_tag;
    use ntest::timeout;
    use std::env;
    use std::str::FromStr;
    use tokio::task::JoinHandle;

    use sui_config::SUI_KEYSTORE_FILENAME;
    use sui_indexer::errors::IndexerError;
    use sui_indexer::models::objects::{
        compose_object_bulk_insert_query, compose_object_bulk_insert_update_query,
        group_and_sort_objects, NamedBcsBytes, Object, ObjectStatus,
    };
    use sui_indexer::models::owners::OwnerType;
    use sui_indexer::schema::objects;
    use sui_indexer::store::{IndexerStore, PgIndexerStore};
    use sui_indexer::test_utils::{start_test_indexer, SuiTransactionBlockResponseBuilder};
    use sui_indexer::{get_pg_pool_connection, new_pg_connection_pool, IndexerConfig};
    use sui_json_rpc::api::ExtendedApiClient;
    use sui_json_rpc::api::IndexerApiClient;
    use sui_json_rpc::api::{ReadApiClient, TransactionBuilderClient, WriteApiClient};
    use sui_json_rpc_types::{
        CheckpointId, EventFilter, SuiMoveObject, SuiObjectData, SuiObjectDataFilter,
        SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery, SuiParsedMoveObject,
        SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
        SuiTransactionBlockResponseQuery, TransactionBlockBytes,
    };
    use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
    use sui_types::base_types::{ObjectID, SuiAddress};
    use sui_types::digests::{ObjectDigest, TransactionDigest};
    use sui_types::error::SuiObjectResponseError;
    use sui_types::gas_coin::GasCoin;
    use sui_types::messages::{ExecuteTransactionRequestType, TEST_ONLY_GAS_UNIT_FOR_TRANSFER};
    use sui_types::object::ObjectFormatOptions;
    use sui_types::query::TransactionFilter;
    use sui_types::utils::to_sender_signed_transaction;
    use test_utils::network::{TestCluster, TestClusterBuilder};
    use test_utils::transaction::{create_devnet_nft, delete_devnet_nft, publish_nfts_package};

    const WAIT_UNTIL_TIME_LIMIT: u64 = 60;

    async fn get_owned_objects_for_address(
        indexer_rpc_client: &HttpClient,
        address: &SuiAddress,
    ) -> Result<Vec<ObjectID>, anyhow::Error> {
        let gas_objects: Vec<ObjectID> = indexer_rpc_client
            .get_owned_objects(
                *address,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::new().with_type(),
                )),
                None,
                None,
            )
            .await?
            .data
            .into_iter()
            .filter_map(|object_resp| {
                if let Some(data) = object_resp.data {
                    Some(data.object_id)
                } else {
                    None
                }
            })
            .collect();

        Ok(gas_objects)
    }

    async fn sign_and_execute_transaction_block(
        test_cluster: &TestCluster,
        indexer_rpc_client: &HttpClient,
        transaction_bytes: TransactionBlockBytes,
        sender: &SuiAddress,
    ) -> Result<SuiTransactionBlockResponse, anyhow::Error> {
        let keystore_path = test_cluster.swarm.dir().join(SUI_KEYSTORE_FILENAME);
        let keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
        let tx =
            to_sender_signed_transaction(transaction_bytes.to_data()?, keystore.get_key(sender)?);
        let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();
        let tx_response = indexer_rpc_client
            .execute_transaction_block(
                tx_bytes,
                signatures,
                Some(SuiTransactionBlockResponseOptions::full_content()),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(tx_response)
    }

    async fn sign_and_transfer_object(
        test_cluster: &TestCluster,
        indexer_rpc_client: &HttpClient,
        sender: &SuiAddress,
        recipient: &SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
    ) -> Result<SuiTransactionBlockResponse, anyhow::Error> {
        let rgp = test_cluster.get_reference_gas_price().await;
        let transaction_bytes: TransactionBlockBytes = indexer_rpc_client
            .transfer_object(
                *sender,
                object_id,
                gas,
                (rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER).into(),
                *recipient,
            )
            .await?;
        let tx_response = sign_and_execute_transaction_block(
            test_cluster,
            indexer_rpc_client,
            transaction_bytes,
            sender,
        )
        .await?;
        Ok(tx_response)
    }

    // TODO: we should use SuiClient for tests like this
    async fn execute_simple_transfer(
        test_cluster: &mut TestCluster,
        indexer_rpc_client: &HttpClient,
    ) -> Result<
        (
            SuiTransactionBlockResponse,
            SuiAddress,
            SuiAddress,
            Vec<ObjectID>,
        ),
        anyhow::Error,
    > {
        let sender = test_cluster.accounts.first().unwrap();
        let recipient = test_cluster.accounts.last().unwrap();
        // TODO(gegaowp): today indexer's get_owned_objects only supports filter
        // by owner address, will revert this when the feature is complete.
        let gas_objects: Vec<ObjectID> = test_cluster
            .rpc_client()
            .get_owned_objects(
                *sender,
                Some(SuiObjectResponseQuery::new_with_filter(
                    SuiObjectDataFilter::gas_coin(),
                )),
                None,
                None,
            )
            .await?
            .data
            .into_iter()
            .filter_map(|object_resp| {
                if let Some(data) = object_resp.data {
                    Some(data.object_id)
                } else {
                    None
                }
            })
            .collect();

        let tx_response = sign_and_transfer_object(
            test_cluster,
            indexer_rpc_client,
            sender,
            recipient,
            *gas_objects.first().unwrap(),
            Some(*gas_objects.last().unwrap()),
        )
        .await?;

        Ok((tx_response, *sender, *recipient, gas_objects))
    }

    #[tokio::test]
    #[timeout(60000)]
    async fn test_genesis_sync() {
        let (test_cluster, indexer_rpc_client, store, handle) = start_test_cluster(None).await;
        // Allow indexer to sync
        wait_until_next_checkpoint(&store).await;

        let checkpoint = store.get_checkpoint(0.into()).await.unwrap();

        for tx_digest in checkpoint.transactions {
            let transaction = store
                .get_transaction_by_digest(&tx_digest.base58_encode())
                .await;
            assert!(transaction.is_ok());
            let _fullnode_rpc_tx = test_cluster
                .rpc_client()
                .get_transaction_block(tx_digest, Some(SuiTransactionBlockResponseOptions::new()))
                .await
                .unwrap();
            let _indexer_rpc_tx = indexer_rpc_client
                .get_transaction_block(tx_digest, Some(SuiTransactionBlockResponseOptions::new()))
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
    #[timeout(60000)]
    async fn test_total_addresses() -> Result<(), anyhow::Error> {
        let (_test_cluster, _, store, _handle) = start_test_cluster(None).await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;
        let total_address_count = store.get_network_metrics().await.unwrap().total_addresses;
        assert_eq!(10, total_address_count);
        Ok(())
    }

    #[ignore]
    #[tokio::test]
    async fn test_total_objects() -> Result<(), anyhow::Error> {
        let (_test_cluster, _, store, _handle) = start_test_cluster(None).await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;
        let total_object_count = store.get_network_metrics().await.unwrap().total_objects;
        assert_eq!(48, total_object_count);
        Ok(())
    }

    #[tokio::test]
    async fn test_total_packages() -> Result<(), anyhow::Error> {
        let (_test_cluster, _, store, _handle) = start_test_cluster(None).await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;
        let total_package_count = store.get_network_metrics().await.unwrap().total_packages;
        assert_eq!(3, total_package_count);
        Ok(())
    }

    #[tokio::test]
    async fn test_total_transaction() -> Result<(), anyhow::Error> {
        let (mut test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster(None).await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;
        let (tx_response, _, _, _) =
            execute_simple_transfer(&mut test_cluster, &indexer_rpc_client).await?;
        wait_until_transaction_synced_in_checkpoint(
            &store,
            tx_response.digest.base58_encode().as_str(),
        )
        .await;
        let tx_count = store
            .get_total_transaction_number_from_checkpoints()
            .await
            .unwrap();
        // At least 1 transaction + 1 genesis, others are like Consensus Commit Prologue
        assert!(tx_count >= 2);
        let rpc_tx_count = indexer_rpc_client
            .get_total_transaction_blocks()
            .await
            .unwrap();
        assert!(*rpc_tx_count >= 2);
        Ok(())
    }

    #[tokio::test]
    #[timeout(60000)]
    async fn test_simple_transaction_e2e() -> Result<(), anyhow::Error> {
        let (mut test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster(None).await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;
        let (package_id, _, _, publish_digest) =
            publish_nfts_package(&mut test_cluster.wallet).await;
        wait_until_transaction_synced(&store, publish_digest.base58_encode().as_str()).await;
        wait_until_next_checkpoint(&store).await;

        let (tx_response, sender, recipient, gas_objects) =
            execute_simple_transfer(&mut test_cluster, &indexer_rpc_client).await?;

        wait_until_transaction_synced(&store, tx_response.digest.base58_encode().as_str()).await;
        let (_, _, nft_digest) = create_devnet_nft(&mut test_cluster.wallet, package_id)
            .await
            .unwrap();
        wait_until_transaction_synced(&store, nft_digest.base58_encode().as_str()).await;
        wait_until_next_checkpoint(&store).await;

        // query tx with checkpoint sequence
        let checkpoint_seq_query =
            SuiTransactionBlockResponseQuery::new_with_filter(TransactionFilter::Checkpoint(2u64));
        let mut checkpoint_query_tx_digest_vec = indexer_rpc_client
            .query_transaction_blocks(checkpoint_seq_query, None, None, None)
            .await
            .unwrap()
            .data
            .into_iter()
            .map(|tx| tx.digest)
            .collect::<Vec<_>>();
        let mut checkpoint_tx_digest_vec = indexer_rpc_client
            .get_checkpoint(CheckpointId::SequenceNumber(2u64))
            .await
            .unwrap()
            .transactions;
        checkpoint_query_tx_digest_vec.sort();
        checkpoint_tx_digest_vec.sort();
        assert_eq!(checkpoint_query_tx_digest_vec, checkpoint_tx_digest_vec);

        let tx_read_response = indexer_rpc_client
            .get_transaction_block(
                tx_response.digest,
                Some(SuiTransactionBlockResponseOptions::full_content()),
            )
            .await?;
        assert_eq!(tx_response.digest, tx_read_response.digest);
        assert_eq!(tx_response.transaction, tx_read_response.transaction);
        assert_eq!(tx_response.effects, tx_read_response.effects);
        assert_eq!(tx_response.events, tx_read_response.events);
        assert_eq!(tx_response.object_changes, tx_read_response.object_changes);
        assert_eq!(
            tx_response.balance_changes,
            tx_read_response.balance_changes
        );
        wait_until_next_checkpoint(&store).await;

        // query tx with sender address
        let from_query = SuiTransactionBlockResponseQuery::new_with_filter(
            TransactionFilter::FromAddress(sender),
        );
        let tx_from_query_response = indexer_rpc_client
            .query_transaction_blocks(from_query, None, None, None)
            .await?;
        assert!(!tx_from_query_response.has_next_page);
        assert_eq!(tx_from_query_response.data.len(), 3);

        let tx_kind_query = SuiTransactionBlockResponseQuery::new_with_filter(
            TransactionFilter::TransactionKind("ProgrammableTransaction".to_string()),
        );
        let tx_kind_query_response = indexer_rpc_client
            .query_transaction_blocks(tx_kind_query, None, None, None)
            .await?;
        assert!(!tx_kind_query_response.has_next_page);
        assert_eq!(tx_kind_query_response.data.len(), 3);

        // query tx with recipient address
        let to_query = SuiTransactionBlockResponseQuery::new_with_filter(
            TransactionFilter::ToAddress(recipient),
        );
        let tx_to_query_response = indexer_rpc_client
            .query_transaction_blocks(to_query, None, None, None)
            .await?;
        // the address has received 2 transactions, one is genesis
        assert!(!tx_to_query_response.has_next_page);
        assert_eq!(tx_to_query_response.data.len(), 2);

        // query tx with both sender and recipient addresses
        let from_to_query = SuiTransactionBlockResponseQuery::new_with_filter(
            TransactionFilter::FromAndToAddress {
                from: sender,
                to: recipient,
            },
        );
        let tx_from_to_query_response = indexer_rpc_client
            .query_transaction_blocks(from_to_query, None, None, None)
            .await?;
        assert!(!tx_from_to_query_response.has_next_page);
        assert_eq!(tx_from_to_query_response.data.len(), 1);
        assert_eq!(
            tx_response.digest,
            tx_from_to_query_response.data.first().unwrap().digest
        );

        // query tx with mutated object id
        let mutation_query = SuiTransactionBlockResponseQuery::new_with_filter(
            TransactionFilter::ChangedObject(*gas_objects.first().unwrap()),
        );
        let tx_mutation_query_response = indexer_rpc_client
            .query_transaction_blocks(mutation_query, None, None, None)
            .await?;
        // the coin is first created by genesis tx, then transferred by the above tx
        assert!(!tx_mutation_query_response.has_next_page);
        assert_eq!(tx_mutation_query_response.data.len(), 3);

        // query tx with input object id
        let input_query = SuiTransactionBlockResponseQuery::new_with_filter(
            TransactionFilter::InputObject(*gas_objects.first().unwrap()),
        );
        let tx_input_query_response = indexer_rpc_client
            .query_transaction_blocks(input_query, None, None, None)
            .await?;
        assert_eq!(tx_input_query_response.data.len(), 2);

        // query tx with move call
        let move_call_query =
            SuiTransactionBlockResponseQuery::new_with_filter(TransactionFilter::MoveFunction {
                package: package_id,
                module: Some("devnet_nft".to_string()),
                function: None,
            });
        let tx_move_call_query_response = indexer_rpc_client
            .query_transaction_blocks(move_call_query, None, None, None)
            .await?;
        assert_eq!(tx_move_call_query_response.data.len(), 1);
        assert_eq!(
            tx_move_call_query_response.data.first().unwrap().digest,
            nft_digest
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_multi_get_transactions_order() -> Result<(), anyhow::Error> {
        let (mut test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster(None).await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;
        let (package_id, _, _, publish_digest) =
            publish_nfts_package(&mut test_cluster.wallet).await;
        wait_until_transaction_synced(&store, publish_digest.base58_encode().as_str()).await;
        let (tx_response, _, _, _) =
            execute_simple_transfer(&mut test_cluster, &indexer_rpc_client).await?;
        wait_until_transaction_synced(&store, tx_response.digest.base58_encode().as_str()).await;
        let (_, _, nft_digest) = create_devnet_nft(&mut test_cluster.wallet, package_id)
            .await
            .unwrap();
        wait_until_transaction_synced(&store, nft_digest.base58_encode().as_str()).await;

        let tx_multi_read_tx_response_1 = indexer_rpc_client
            .multi_get_transaction_blocks(
                vec![tx_response.digest, nft_digest],
                Some(SuiTransactionBlockResponseOptions::full_content()),
            )
            .await?;
        assert_eq!(tx_multi_read_tx_response_1.len(), 2);
        assert_eq!(tx_multi_read_tx_response_1[0].digest, tx_response.digest);
        assert_eq!(tx_multi_read_tx_response_1[1].digest, nft_digest);

        let tx_multi_read_tx_response_2 = indexer_rpc_client
            .multi_get_transaction_blocks(
                vec![nft_digest, tx_response.digest],
                Some(SuiTransactionBlockResponseOptions::full_content()),
            )
            .await?;
        assert_eq!(tx_multi_read_tx_response_2.len(), 2);
        assert_eq!(tx_multi_read_tx_response_2[0].digest, nft_digest);
        assert_eq!(tx_multi_read_tx_response_2[1].digest, tx_response.digest);

        Ok(())
    }

    #[tokio::test]
    #[timeout(60000)]
    async fn test_event_query_e2e() -> Result<(), anyhow::Error> {
        let (mut test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster(None).await;
        wait_until_next_checkpoint(&store).await;
        let nft_creator = test_cluster.get_address_0();
        let context = &mut test_cluster.wallet;
        let (package_id, _, _, publish_digest) = publish_nfts_package(context).await;
        wait_until_transaction_synced(&store, publish_digest.base58_encode().as_str()).await;

        let (_, _, digest_one) = create_devnet_nft(context, package_id).await.unwrap();
        wait_until_transaction_synced(&store, digest_one.base58_encode().as_str()).await;
        let (sender, _, digest_two) = create_devnet_nft(context, package_id).await.unwrap();
        wait_until_transaction_synced(&store, digest_two.base58_encode().as_str()).await;

        // Test various ways of querying events
        let filter_on_sender = EventFilter::Sender(sender);
        let query_response = indexer_rpc_client
            .query_events(filter_on_sender, None, None, None)
            .await?;
        let target_struct_tag =
            StructTag::from_str(&format!("{package_id}::devnet_nft::MintNFTEvent")).unwrap();

        assert_eq!(query_response.data.len(), 2);
        for item in query_response.data {
            assert_eq!(item.transaction_module, ident_str!("devnet_nft").into());
            assert_eq!(item.package_id, package_id);
            assert_eq!(item.sender, nft_creator);
            assert_eq!(item.type_, target_struct_tag.clone());
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
            package: package_id,
            module: Identifier::new("devnet_nft").unwrap(),
        };
        let query_response = indexer_rpc_client
            .query_events(filter_on_module, None, None, None)
            .await?;
        assert_eq!(query_response.data.len(), 2);
        assert_eq!(digest_one, query_response.data[0].id.tx_digest);
        assert_eq!(digest_two, query_response.data[1].id.tx_digest);

        let filter_on_event_type = EventFilter::MoveEventType(target_struct_tag.clone());
        let query_response = indexer_rpc_client
            .query_events(filter_on_event_type.clone(), None, None, None)
            .await?;
        assert_eq!(query_response.data.len(), 2);
        assert_eq!(digest_one, query_response.data[0].id.tx_digest);
        assert_eq!(digest_two, query_response.data[1].id.tx_digest);

        // check parsed event data with FN
        let fn_query_response = test_cluster
            .rpc_client()
            .query_events(filter_on_event_type, None, None, None)
            .await?;

        assert_eq!(fn_query_response.data.len(), 2);
        assert_eq!(digest_one, fn_query_response.data[0].id.tx_digest);
        assert_eq!(digest_two, fn_query_response.data[1].id.tx_digest);

        assert_eq!(
            query_response.data[0].parsed_json,
            fn_query_response.data[0].parsed_json
        );
        Ok(())
    }

    #[tokio::test]
    #[timeout(60000)]
    async fn test_event_query_pagination_e2e() -> Result<(), anyhow::Error> {
        let (mut test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster(None).await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;
        let context = &mut test_cluster.wallet;
        let (package_id, _, _, publish_digest) = publish_nfts_package(context).await;
        wait_until_transaction_synced(&store, publish_digest.base58_encode().as_str()).await;

        for _ in 0..5 {
            let (sender, object_id, digest) = create_devnet_nft(context, package_id).await.unwrap();
            wait_until_transaction_synced(&store, digest.base58_encode().as_str()).await;
            let obj_resp = indexer_rpc_client
                .get_object(object_id, None)
                .await
                .unwrap();
            let data = obj_resp.object()?;
            let result = delete_devnet_nft(
                context,
                &sender,
                package_id,
                (data.object_id, data.version, data.digest),
            )
            .await;
            wait_until_transaction_synced(&store, result.digest.base58_encode().as_str()).await;
        }

        let filter_on_module = EventFilter::MoveModule {
            package: package_id,
            module: Identifier::new("devnet_nft").unwrap(),
        };
        let query_response = indexer_rpc_client
            .query_events(filter_on_module, None, None, None)
            .await?;
        assert_eq!(query_response.data.len(), 5);

        let mint_nft_event = &format!("{package_id}::devnet_nft::MintNFTEvent");
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
        let burn_nft_event = &format!("{package_id}::devnet_nft::BurnNFTEvent");
        let filter = get_filter_on_event_type(burn_nft_event);
        let query_response = indexer_rpc_client
            .query_events(filter, None, Some(4), None)
            .await?;
        assert!(!query_response.has_next_page);
        assert_eq!(query_response.data.len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_object_with_options() -> Result<(), anyhow::Error> {
        let (test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster(None).await;
        wait_until_next_checkpoint(&store).await;
        let address = test_cluster.get_address_0();
        let gas_objects = get_owned_objects_for_address(&indexer_rpc_client, &address).await?;
        let source_object_id = *gas_objects.first().unwrap();
        let show_all_content = SuiObjectDataOptions {
            show_type: true,
            show_owner: true,
            show_previous_transaction: true,
            show_display: true,
            show_content: true,
            show_bcs: true,
            show_storage_rebate: true,
        };
        let resp = indexer_rpc_client
            .get_object(source_object_id, Some(show_all_content.clone()))
            .await
            .unwrap();
        let initial_full_obj_data = resp.object()?;
        let tx_response = sign_and_transfer_object(
            &test_cluster,
            &indexer_rpc_client,
            &test_cluster.get_address_0(),
            &test_cluster.get_address_1(),
            source_object_id,
            None,
        )
        .await?;
        wait_until_transaction_synced_in_checkpoint(
            &store,
            tx_response.digest.base58_encode().as_str(),
        )
        .await;
        let response = indexer_rpc_client
            .get_object(source_object_id, Some(show_all_content.clone()))
            .await?;
        let post_transfer_full_obj_data = response.object()?;
        let object_required_fields = SuiObjectData {
            type_: None,
            owner: None,
            previous_transaction: None,
            storage_rebate: None,
            display: None,
            content: None,
            bcs: None,
            ..post_transfer_full_obj_data.clone()
        };
        let show_some_content = SuiObjectDataOptions::new();
        let futures = vec![
            indexer_rpc_client
                .get_object(source_object_id, Some(SuiObjectDataOptions::bcs_lossless())),
            indexer_rpc_client
                .get_object(source_object_id, Some(SuiObjectDataOptions::full_content())),
            indexer_rpc_client.get_object(source_object_id, Some(show_some_content.clone())),
            indexer_rpc_client.get_object(
                source_object_id,
                Some(show_some_content.clone().with_content()),
            ),
            indexer_rpc_client.get_object(
                source_object_id,
                Some(show_some_content.clone().with_owner()),
            ),
            indexer_rpc_client.get_object(
                source_object_id,
                Some(show_some_content.clone().with_type()),
            ),
            indexer_rpc_client.get_object(
                source_object_id,
                Some(show_some_content.clone().with_display()),
            ),
            indexer_rpc_client
                .get_object(source_object_id, Some(show_some_content.clone().with_bcs())),
            indexer_rpc_client.get_object(
                source_object_id,
                Some(show_some_content.clone().with_previous_transaction()),
            ),
        ];

        let results: Vec<SuiObjectResponse> = join_all(futures)
            .await
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap();

        let expected_results = vec![
            // bcs_lossless
            SuiObjectData {
                display: None,
                content: None,
                ..post_transfer_full_obj_data.clone()
            },
            // full_content
            SuiObjectData {
                bcs: None,
                display: None,
                ..post_transfer_full_obj_data.clone()
            },
            // non-optional
            object_required_fields.clone(),
            SuiObjectData {
                content: post_transfer_full_obj_data.content.clone(),
                ..object_required_fields.clone()
            },
            SuiObjectData {
                owner: post_transfer_full_obj_data.owner,
                ..object_required_fields.clone()
            },
            SuiObjectData {
                type_: post_transfer_full_obj_data.type_.clone(),
                ..object_required_fields.clone()
            },
            SuiObjectData {
                display: post_transfer_full_obj_data.display.clone(),
                ..object_required_fields.clone()
            },
            SuiObjectData {
                bcs: post_transfer_full_obj_data.bcs.clone(),
                ..object_required_fields.clone()
            },
            SuiObjectData {
                previous_transaction: post_transfer_full_obj_data.previous_transaction,
                ..object_required_fields.clone()
            },
        ];

        for (received, expected) in results.iter().zip(expected_results.iter()) {
            let data = received.object()?;
            assert_eq!(data, expected);
            assert_eq!(data.version.value(), 2);
            assert_eq!(data.object_id, initial_full_obj_data.object_id);
        }

        // deleted object - returns SuiObjectRef
        let gas_objects =
            get_owned_objects_for_address(&indexer_rpc_client, &test_cluster.get_address_1())
                .await?;
        let primary_coin = gas_objects
            .iter()
            .find(|&id| *id != post_transfer_full_obj_data.object_id)
            .unwrap();
        assert_ne!(*primary_coin, post_transfer_full_obj_data.object_id);

        let transaction_bytes = indexer_rpc_client
            .merge_coin(
                test_cluster.get_address_1(),
                *primary_coin,                         // coin to merge into
                post_transfer_full_obj_data.object_id, // coin to merge and delete
                None,
                2_000_000.into(),
            )
            .await?;
        let tx_response = sign_and_execute_transaction_block(
            &test_cluster,
            &indexer_rpc_client,
            transaction_bytes,
            &test_cluster.get_address_1(),
        )
        .await?;
        wait_until_transaction_synced_in_checkpoint(
            &store,
            tx_response.digest.base58_encode().as_str(),
        )
        .await;
        wait_until_next_checkpoint(&store).await;

        let resp = indexer_rpc_client
            .get_object(post_transfer_full_obj_data.object_id, None)
            .await
            .unwrap();

        match (&resp.data, &resp.error) {
            (
                None,
                Some(SuiObjectResponseError::Deleted {
                    object_id,
                    version,
                    digest,
                }),
            ) => {
                assert_eq!(object_id, &post_transfer_full_obj_data.object_id);
                assert_eq!(digest, &post_transfer_full_obj_data.digest);
                assert_eq!(version.value(), 3);
            }
            _ => {
                panic!(
                    "Expected SuiObjectResponse::Deleted, but got {:?}",
                    resp.error
                );
            }
        }

        // Not exists
        let obj_id = ObjectID::new([42; 32]);
        let resp = indexer_rpc_client
            .get_object(obj_id, Some(show_all_content.clone()))
            .await
            .unwrap();

        if let Some(SuiObjectResponseError::NotExists { object_id }) = resp.error {
            assert_eq!(object_id, obj_id)
        } else {
            panic!(
                "Expected SuiObjectResponse::NotExists, but got {:?}",
                resp.error
            );
        }

        Ok(())
    }

    #[tokio::test]
    #[timeout(60000)]
    async fn test_module_cache() {
        let (test_cluster, _, store, handle) = start_test_cluster(None).await;
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
            .await
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

    #[tokio::test]
    #[timeout(60000)]
    async fn test_get_epoch() {
        let (test_cluster, _, store, handle) = start_test_cluster(Some(10000)).await;

        // Allow indexer to sync
        wait_until_next_checkpoint(&store).await;

        let current_epoch = store.get_current_epoch().await.unwrap();
        let epoch_page = store.get_epochs(None, 100, None).await.unwrap();
        assert_eq!(0, current_epoch.epoch);
        assert!(current_epoch.end_of_epoch_info.is_none());
        assert_eq!(1, epoch_page.len());
        wait_until_next_epoch(&store).await;

        let current_epoch = store.get_current_epoch().await.unwrap();
        let epoch_page = store.get_epochs(None, 100, None).await.unwrap();

        assert_eq!(1, current_epoch.epoch);
        assert!(current_epoch.end_of_epoch_info.is_none());
        assert_eq!(2, epoch_page.len());

        let last_epoch = &epoch_page[0];
        assert!(last_epoch.end_of_epoch_info.is_some());

        drop(handle);
        drop(test_cluster);
    }

    #[tokio::test]
    #[timeout(60000)]
    async fn test_get_last_checkpoint_of_epoch() {
        let (test_cluster, _, store, handle) = start_test_cluster(Some(20000)).await;
        // Allow indexer to sync geneis epoch
        wait_until_next_checkpoint(&store).await;
        wait_until_next_epoch(&store).await;
        let current_epoch = store.get_current_epoch().await.unwrap();
        let prev_epoch_last_checkpoint_id = current_epoch.first_checkpoint_id - 1;
        wait_for_checkpoint(&store, current_epoch.first_checkpoint_id as i64).await;

        let checkpoint = store
            .get_checkpoint(CheckpointId::SequenceNumber(prev_epoch_last_checkpoint_id))
            .await
            .unwrap();
        assert_eq!(checkpoint.epoch as u64, current_epoch.epoch - 1);
        assert_eq!(checkpoint.sequence_number, prev_epoch_last_checkpoint_id);
        assert!(checkpoint.end_of_epoch_data.is_some());

        assert_eq!(checkpoint.epoch, current_epoch.epoch - 1);
        assert_eq!(checkpoint.sequence_number, prev_epoch_last_checkpoint_id);

        // cross check with FN
        let fn_cp = test_cluster
            .rpc_client()
            .get_checkpoint(CheckpointId::SequenceNumber(prev_epoch_last_checkpoint_id))
            .await
            .unwrap();

        assert_eq!(fn_cp, checkpoint);

        drop(handle);
        drop(test_cluster);
    }

    #[tokio::test]
    #[timeout(60000)]
    async fn test_query_objects_cross_check() -> Result<(), anyhow::Error> {
        let (test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster(None).await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;
        let address = test_cluster.accounts[0];
        let fullnode_client = test_cluster.rpc_client();

        let object_from_fullnode = fullnode_client
            .get_owned_objects(address, None, None, None)
            .await
            .unwrap();

        let object_from_indexer = indexer_rpc_client
            .query_objects(
                SuiObjectResponseQuery::new_with_filter(SuiObjectDataFilter::AddressOwner(address)),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(object_from_fullnode.data, object_from_indexer.data);
        Ok(())
    }

    #[tokio::test]
    #[timeout(60000)]
    async fn test_query_objects() -> Result<(), anyhow::Error> {
        let (_test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster(None).await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;

        let all_coins = indexer_rpc_client
            .query_objects(
                SuiObjectResponseQuery::new_with_filter(SuiObjectDataFilter::StructType(
                    parse_struct_tag("0x2::coin::Coin<0x2::sui::SUI>").unwrap(),
                )),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(25, all_coins.data.len());
        Ok(())
    }

    #[tokio::test]
    #[timeout(60000)]
    async fn pg_parameter_limit_test() {
        // Helps clear/build the database
        start_test_cluster(None).await;

        let pg_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into());
        let pg_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "32770".into());
        let pw = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgrespw".into());
        let db_url = format!("postgres://postgres:{pw}@{pg_host}:{pg_port}");
        let (pg_connection_pool, _) = new_pg_connection_pool(&db_url).await.unwrap();
        let mut pg_pool_conn = get_pg_pool_connection(&pg_connection_pool).unwrap();

        let lot_of_data = (1..10000)
            .into_iter()
            .map(|_| Object {
                epoch: 0,
                checkpoint: 0,
                object_id: ObjectID::random().to_string(),
                version: 0,
                object_digest: "".to_string(),
                owner_type: OwnerType::AddressOwner,
                owner_address: None,
                initial_shared_version: None,
                previous_transaction: "".to_string(),
                object_type: "".to_string(),
                object_status: ObjectStatus::Created,
                has_public_transfer: false,
                storage_rebate: 0,
                bcs: vec![],
            })
            .collect::<Vec<_>>();

        // this should fail because of the parameter limit
        let result = pg_pool_conn
            .build_transaction()
            .serializable()
            .read_write()
            .run(|conn| {
                diesel::insert_into(objects::table)
                    .values(&lot_of_data)
                    .on_conflict_do_nothing()
                    .execute(conn)
            });

        assert!(result.is_err());

        // this should pass, we can chunk up the data but they can still be transactional.
        let result: Result<(), IndexerError> = pg_pool_conn
            .build_transaction()
            .serializable()
            .read_write()
            .run(|conn| {
                for chunk in lot_of_data.chunks(1000) {
                    diesel::insert_into(objects::table)
                        .values(chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)?;
                }
                Ok(())
            });
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[timeout(60000)]
    async fn pg_unnest_bulk_insert_update_test() {
        start_test_cluster(None).await;
        let pg_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into());
        let pg_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "32770".into());
        let pw = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgrespw".into());
        let db_url = format!("postgres://postgres:{pw}@{pg_host}:{pg_port}");
        let (pg_connection_pool, _) = new_pg_connection_pool(&db_url).await.unwrap();
        let mut pg_pool_conn = get_pg_pool_connection(&pg_connection_pool).unwrap();

        let bulk_data = (1..=10000)
            .into_iter()
            .map(|_| Object {
                epoch: 0,
                checkpoint: 0,
                object_id: ObjectID::random().to_string(),
                version: 0,
                object_digest: ObjectDigest::random().to_string(),
                owner_type: OwnerType::AddressOwner,
                owner_address: Some(SuiAddress::random_for_testing_only().to_string()),
                initial_shared_version: None,
                previous_transaction: TransactionDigest::random().to_string(),
                object_type: "0x2::coin::Coin<0x2::sui::SUI>".to_string(),
                object_status: ObjectStatus::Created,
                has_public_transfer: false,
                storage_rebate: 0,
                bcs: vec![NamedBcsBytes("object".to_string(), vec![1u8, 2u8, 3u8])],
            })
            .collect::<Vec<_>>();

        let insert_query = compose_object_bulk_insert_query(&bulk_data);
        let result: Result<usize, IndexerError> = pg_pool_conn
            .build_transaction()
            .serializable()
            .read_write()
            .run(|conn| diesel::sql_query(insert_query).execute(conn))
            .map_err(IndexerError::PostgresError);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 10000);

        let mutated_bulk_data = bulk_data
            .clone()
            .into_iter()
            .map(|mut object| {
                object.object_status = ObjectStatus::Mutated;
                object.version = 1;
                object
            })
            .collect::<Vec<_>>();
        let mutated_bulk_data_same_checkpoint = bulk_data
            .into_iter()
            .map(|mut object| {
                object.object_status = ObjectStatus::Mutated;
                object.version = 2;
                object
            })
            .chain(mutated_bulk_data)
            .collect::<Vec<_>>();
        let mut pg_pool_conn = get_pg_pool_connection(&pg_connection_pool).unwrap();
        let mut mutated_object_groups = group_and_sort_objects(mutated_bulk_data_same_checkpoint);
        let mut counter = 0;
        loop {
            let mutated_object_group = mutated_object_groups
                .iter_mut()
                .filter_map(|group| group.pop())
                .collect::<Vec<_>>();
            if mutated_object_group.is_empty() {
                break;
            }
            // bulk insert/update via UNNEST trick
            let insert_update_query =
                compose_object_bulk_insert_update_query(&mutated_object_group);
            let result: Result<usize, IndexerError> = pg_pool_conn
                .build_transaction()
                .serializable()
                .read_write()
                .run(|conn| diesel::sql_query(insert_update_query).execute(conn))
                .map_err(IndexerError::PostgresError);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), 10000);
            counter += 1;
        }
        assert_eq!(counter, 2);
    }

    #[tokio::test]
    async fn test_get_transaction_with_options() -> Result<(), anyhow::Error> {
        let (mut test_cluster, indexer_rpc_client, store, _handle) = start_test_cluster(None).await;
        // Allow indexer to sync genesis
        wait_until_next_checkpoint(&store).await;
        let (tx_response, _, _, _) =
            execute_simple_transfer(&mut test_cluster, &indexer_rpc_client).await?;
        wait_until_transaction_synced_in_checkpoint(
            &store,
            tx_response.digest.base58_encode().as_str(),
        )
        .await;
        let full_transaction_response = indexer_rpc_client
            .get_transaction_block(
                tx_response.digest,
                Some(SuiTransactionBlockResponseOptions::full_content()),
            )
            .await?;
        let sui_transaction_response_options = vec![
            SuiTransactionBlockResponseOptions::new().with_input(),
            SuiTransactionBlockResponseOptions::new().with_raw_input(),
            SuiTransactionBlockResponseOptions::new().with_effects(),
            SuiTransactionBlockResponseOptions::new().with_events(),
            SuiTransactionBlockResponseOptions::new().with_balance_changes(),
            SuiTransactionBlockResponseOptions::new().with_object_changes(),
            SuiTransactionBlockResponseOptions::new()
                .with_input()
                .with_balance_changes()
                .with_object_changes(),
        ];
        let futures = sui_transaction_response_options
            .into_iter()
            .map(|option| {
                indexer_rpc_client.get_transaction_block(tx_response.digest, Some(option))
            })
            .collect::<Vec<_>>();

        let received_transaction_results: Vec<SuiTransactionBlockResponse> = join_all(futures)
            .await
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap();

        let expected_transaction_results = vec![
            SuiTransactionBlockResponseBuilder::new(&full_transaction_response)
                .with_input()
                .build(),
            SuiTransactionBlockResponseBuilder::new(&full_transaction_response)
                .with_raw_input()
                .build(),
            SuiTransactionBlockResponseBuilder::new(&full_transaction_response)
                .with_effects()
                .build(),
            SuiTransactionBlockResponseBuilder::new(&full_transaction_response)
                .with_events()
                .build(),
            SuiTransactionBlockResponseBuilder::new(&full_transaction_response)
                .with_balance_changes()
                .build(),
            SuiTransactionBlockResponseBuilder::new(&full_transaction_response)
                .with_object_changes()
                .build(),
            SuiTransactionBlockResponseBuilder::new(&full_transaction_response)
                .with_input()
                .with_balance_changes()
                .with_object_changes()
                .build(),
        ];
        for (i, (received, expected)) in received_transaction_results
            .iter()
            .zip(expected_transaction_results.iter())
            .enumerate()
        {
            assert_eq!(received, expected, "Mismatch found at index {}", i);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_get_checkpoint() -> Result<(), anyhow::Error> {
        let (mut test_cluster, indexer_rpc_client, store, _) =
            start_test_cluster(Some(20000)).await;
        // Allow indexer to sync
        wait_until_next_checkpoint(&store).await;
        let mut cp_res = store.get_latest_checkpoint_sequence_number().await;
        while cp_res.is_err() {
            cp_res = store.get_latest_checkpoint_sequence_number().await;
        }
        let cp = cp_res.unwrap() as u64;
        let first_checkpoint = indexer_rpc_client
            .get_checkpoint(CheckpointId::SequenceNumber(cp))
            .await
            .unwrap();

        let current_epoch = store.get_current_epoch().await.unwrap();

        assert_eq!(first_checkpoint.epoch, current_epoch.epoch);
        assert_eq!(first_checkpoint.sequence_number, 0);
        assert_eq!(first_checkpoint.network_total_transactions, 1);
        assert_eq!(first_checkpoint.previous_digest, None);
        assert_eq!(first_checkpoint.transactions.len(), 1);

        // Check if checkpoint validator sig matches
        let fullnode_checkpoint = test_cluster
            .rpc_client()
            .get_checkpoint((cp as u64).into())
            .await
            .unwrap();

        assert_eq!(
            first_checkpoint.validator_signature,
            fullnode_checkpoint.validator_signature
        );

        let (tx_response, _, _, _) =
            execute_simple_transfer(&mut test_cluster, &indexer_rpc_client)
                .await
                .unwrap();
        wait_until_transaction_synced_in_checkpoint(
            &store,
            tx_response.digest.base58_encode().as_str(),
        )
        .await;
        // We do this as checkpoint field is only returned in the read api
        let tx_response = indexer_rpc_client
            .get_transaction_block(
                tx_response.digest,
                Some(SuiTransactionBlockResponseOptions::new()),
            )
            .await?;
        let next_cp = tx_response.checkpoint.unwrap();
        let next_checkpoint = indexer_rpc_client
            .get_checkpoint(CheckpointId::SequenceNumber(next_cp))
            .await?;
        let current_epoch = store.get_current_epoch().await.unwrap();

        assert_eq!(next_checkpoint.epoch, current_epoch.epoch);
        assert!(next_checkpoint.sequence_number > first_checkpoint.sequence_number);
        assert!(
            next_checkpoint.network_total_transactions
                > first_checkpoint.network_total_transactions
        );
        assert!(next_checkpoint.transactions.contains(&tx_response.digest));

        let mut curr_checkpoint = next_checkpoint;
        for i in (first_checkpoint.sequence_number..curr_checkpoint.sequence_number).rev() {
            let prev_checkpoint = indexer_rpc_client
                .get_checkpoint(CheckpointId::SequenceNumber(i))
                .await?;
            assert_eq!(
                curr_checkpoint.previous_digest,
                Some(prev_checkpoint.digest)
            );
            curr_checkpoint = prev_checkpoint;
        }
        Ok(())
    }

    async fn start_test_cluster(
        epoch_duration_ms: Option<u64>,
    ) -> (
        TestCluster,
        HttpClient,
        PgIndexerStore,
        JoinHandle<Result<(), IndexerError>>,
    ) {
        let pg_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into());
        let pg_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "32770".into());
        let pw = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgrespw".into());
        let db_url = format!("postgres://postgres:{pw}@{pg_host}:{pg_port}");

        let test_cluster = if let Some(epoch) = epoch_duration_ms {
            TestClusterBuilder::new()
                .with_epoch_duration_ms(epoch)
                .build()
                .await
                .unwrap()
        } else {
            TestClusterBuilder::new().build().await.unwrap()
        };

        let config = IndexerConfig {
            db_url,
            rpc_client_url: test_cluster.rpc_url().to_string(),
            migrated_methods: IndexerConfig::all_implemented_methods(),
            reset_db: true,
            ..Default::default()
        };

        let http_addr_port = format!(
            "http://{}:{}",
            config.rpc_server_url, config.rpc_server_port
        );
        let http_client = HttpClientBuilder::default().build(http_addr_port).unwrap();

        let (store, handle) = start_test_indexer(config).await.unwrap();

        (test_cluster, http_client, store, handle)
    }

    async fn wait_until_next_checkpoint(store: &PgIndexerStore) {
        let since = std::time::Instant::now();
        let mut cp_res = store.get_latest_checkpoint_sequence_number().await;
        while cp_res.is_err() {
            cp_res = store.get_latest_checkpoint_sequence_number().await;
        }
        let mut cp = cp_res.unwrap();
        let target = cp + 1;
        while cp < target {
            let now = std::time::Instant::now();
            if now.duration_since(since).as_secs() > WAIT_UNTIL_TIME_LIMIT {
                panic!("wait_until_next_checkpoint timed out!");
            }
            tokio::task::yield_now().await;
            let mut cp_res = store.get_latest_checkpoint_sequence_number().await;
            while cp_res.is_err() {
                cp_res = store.get_latest_checkpoint_sequence_number().await;
            }
            cp = cp_res.unwrap();
        }
    }

    async fn wait_for_checkpoint(store: &PgIndexerStore, target: i64) {
        let since = std::time::Instant::now();
        let mut cp_res = store.get_latest_checkpoint_sequence_number().await;
        while cp_res.is_err() {
            cp_res = store.get_latest_checkpoint_sequence_number().await;
        }
        let mut cp = cp_res.unwrap();
        while cp < target {
            let now = std::time::Instant::now();
            if now.duration_since(since).as_secs() > WAIT_UNTIL_TIME_LIMIT {
                panic!("wait_for_checkpoint timed out!");
            }
            tokio::task::yield_now().await;
            let mut cp_res = store.get_latest_checkpoint_sequence_number().await;
            while cp_res.is_err() {
                cp_res = store.get_latest_checkpoint_sequence_number().await;
            }
            cp = cp_res.unwrap();
        }
    }

    async fn wait_until_next_epoch(store: &PgIndexerStore) {
        let since = std::time::Instant::now();
        let mut ep = store.get_current_epoch().await.unwrap().epoch;
        let target = ep + 1;
        while ep < target {
            let now = std::time::Instant::now();
            if now.duration_since(since).as_secs() > WAIT_UNTIL_TIME_LIMIT {
                panic!("wait_until_next_epoch timed out!");
            }
            tokio::task::yield_now().await;
            ep = store.get_current_epoch().await.unwrap().epoch;
        }
    }

    async fn wait_until_transaction_synced(store: &PgIndexerStore, tx_digest: &str) {
        let since = std::time::Instant::now();
        let mut tx = store.get_transaction_by_digest(tx_digest).await;
        while tx.is_err() {
            let now = std::time::Instant::now();
            if now.duration_since(since).as_secs() > WAIT_UNTIL_TIME_LIMIT {
                panic!("wait_until_transaction_synced timed out!");
            }
            tokio::task::yield_now().await;
            tx = store.get_transaction_by_digest(tx_digest).await;
        }
    }

    async fn wait_until_transaction_synced_in_checkpoint(store: &PgIndexerStore, tx_digest: &str) {
        let since = std::time::Instant::now();
        loop {
            let tx = store.get_transaction_by_digest(tx_digest).await;
            if let Ok(t) = tx {
                if t.checkpoint_sequence_number.is_some() {
                    break;
                }
            }
            let now = std::time::Instant::now();
            if now.duration_since(since).as_secs() > WAIT_UNTIL_TIME_LIMIT {
                panic!("wait_until_transaction_synced timed out!");
            }
            tokio::task::yield_now().await;
        }
    }

    fn get_filter_on_event_type(event_type: &str) -> EventFilter {
        EventFilter::MoveEventType(StructTag::from_str(event_type).unwrap())
    }
}
