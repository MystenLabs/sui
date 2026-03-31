// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end integration tests for the BigTable KV Store.
//!
//! Each test spawns its own BigTable emulator process on a random port,
//! creates the required tables, and tears everything down when done.
//! Tests require `gcloud`, `cbt`, and the BigTable emulator on PATH.

use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use futures::TryStreamExt;
use prost_types::FieldMask;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_indexer_alt_framework::ingestion::streaming_client::StreamingClientArgs;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_keys::keystore::AccountKeystore;
use sui_kvstore::BigTableClient;
use sui_kvstore::BigTableIndexer;
use sui_kvstore::BigTableStore;
use sui_kvstore::IndexerConfig;
use sui_kvstore::KeyValueStoreReader;
use sui_kvstore::PipelineLayer;
use sui_kvstore::tables::{checkpoints, epochs, transactions};
use sui_kvstore::testing::BigTableEmulator;
use sui_kvstore::testing::INSTANCE_ID;
use sui_kvstore::testing::create_tables;
use sui_kvstore::testing::require_bigtable_emulator;
use sui_protocol_config::Chain;
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::BatchGetTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2::Bcs;
use sui_rpc::proto::sui::rpc::v2::ExecuteTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::Transaction as GrpcTransaction;
use sui_rpc::proto::sui::rpc::v2::UserSignature;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::message_envelope::Message;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::storage::ObjectKey;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use test_cluster::{TestCluster, TestClusterBuilder};
use tokio::time::interval;
use url::Url;

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
            ingestion_config.into(),
            CommitterConfig::default(),
            IndexerConfig::default(),
            PipelineLayer::default(),
            Chain::Unknown,
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

    /// Build, sign, and execute a package publish via gRPC.
    async fn publish_basics_package(&mut self) -> Result<Transaction> {
        let sender = self.cluster.get_address_0();
        let keystore = &self.cluster.wallet.config.keystore;

        let coins = get_all_coins(&mut self.grpc_client, sender).await?;
        let gas_object = coins
            .first()
            .context("No coins available for sender")?
            .compute_object_reference();

        let gas_price = self.cluster.get_reference_gas_price().await;
        let tx_data = TestTransactionBuilder::new(sender, gas_object, gas_price)
            .publish_examples("basics")
            .await
            .build();

        let signed_tx = to_sender_signed_transaction(tx_data, keystore.export(&sender)?);

        grpc_execute_transaction(&mut self.grpc_client, &signed_tx).await?;

        Ok(signed_tx)
    }

    fn bigtable_client(&mut self) -> &mut BigTableClient {
        &mut self.client
    }
}

#[tokio::test]
async fn test_indexer_e2e() -> Result<()> {
    let mut harness = TestHarness::new().await?;

    // -- Publish a package --
    let publish_tx = harness.publish_basics_package().await?;
    let publish_digest = *publish_tx.digest();

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

    // Look up the publish transaction's checkpoint separately so that
    // tx_checkpoints stays aligned with tx_digests (transfers only).
    let publish_cp_resp = harness
        .grpc_client
        .ledger_client()
        .get_transaction(
            GetTransactionRequest::default()
                .with_digest(publish_digest.to_string())
                .with_read_mask(FieldMask::from_paths(["checkpoint"])),
        )
        .await
        .context("get_transaction RPC failed for publish tx")?;
    let publish_cp = publish_cp_resp
        .into_inner()
        .transaction
        .and_then(|t| t.checkpoint)
        .context("publish tx missing checkpoint")?;

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

    let max_checkpoint = *tx_checkpoints.iter().max().unwrap().max(&publish_cp);

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
            .find(|td| td.digest == *signed.digest())
            .unwrap_or_else(|| panic!("transaction {} not found in results", signed.digest()));
        assert_eq!(indexed.transaction().unwrap(), *signed);
        assert!(indexed.checkpoint_number > 0);
        assert!(indexed.timestamp > 0);
    }

    // -- Column-filtered partial reads --
    // Fetch with only effects + checkpoint columns — no td, sg, ev, or bc.
    let partial = harness
        .bigtable_client()
        .get_transactions_filtered(
            &tx_digests,
            Some(&[
                transactions::col::EFFECTS,
                transactions::col::CHECKPOINT_NUMBER,
                transactions::col::TIMESTAMP,
            ]),
        )
        .await?;
    assert_eq!(partial.len(), tx_digests.len());
    for tx in &partial {
        // digest comes from the row key, always present
        assert!(tx_digests.contains(&tx.digest));
        // td was not fetched
        assert!(tx.transaction_data.is_none(), "td should be absent");
        // sg was not fetched
        assert!(tx.signatures.is_none(), "sg should be absent");
        // ef was fetched
        assert!(tx.effects.is_some(), "ef should be present");
        // ev was not fetched
        assert!(tx.events.is_none(), "ev should be absent");
        // bc was not fetched
        assert!(tx.balance_changes.is_empty(), "bc should be empty");
        // metadata always present
        assert!(tx.checkpoint_number > 0);
        assert!(tx.timestamp > 0);
    }

    // -- Balance changes parity with fullnode gRPC batch_get_transactions --
    let batch_response = harness
        .grpc_client
        .ledger_client()
        .batch_get_transactions({
            let mut req = BatchGetTransactionsRequest::default();
            req.digests = tx_digests.iter().map(ToString::to_string).collect();
            req.read_mask = Some(FieldMask::from_paths(["balance_changes"]));
            req
        })
        .await
        .context("batch_get_transactions RPC failed")?
        .into_inner();

    assert_eq!(batch_response.transactions.len(), tx_digests.len());
    for ((digest, indexed), grpc_result) in tx_digests
        .iter()
        .zip(transactions.iter())
        .zip(batch_response.transactions.into_iter())
    {
        let grpc_transaction = grpc_result.to_result().unwrap_or_else(|status| {
            panic!("batch_get_transactions failed for {digest}: {status:?}")
        });

        assert!(
            !indexed.balance_changes.is_empty(),
            "indexed transaction {digest} should contain balance changes"
        );
        assert_eq!(
            grpc_transaction.balance_changes,
            indexed
                .balance_changes
                .iter()
                .cloned()
                .map(Into::into)
                .collect::<Vec<_>>(),
            "balance_changes mismatch for transaction {digest}"
        );
    }

    // -- Unchanged loaded runtime objects --
    let ulro_response = harness
        .grpc_client
        .ledger_client()
        .batch_get_transactions({
            let mut req = BatchGetTransactionsRequest::default();
            req.digests = tx_digests.iter().map(ToString::to_string).collect();
            req.read_mask = Some(FieldMask::from_paths([
                "effects.unchanged_loaded_runtime_objects",
            ]));
            req
        })
        .await
        .context("batch_get_transactions RPC failed for unchanged_loaded")?
        .into_inner();

    assert_eq!(ulro_response.transactions.len(), tx_digests.len());
    for ((digest, indexed), grpc_result) in tx_digests
        .iter()
        .zip(transactions.iter())
        .zip(ulro_response.transactions.into_iter())
    {
        let grpc_transaction = grpc_result.to_result().unwrap_or_else(|status| {
            panic!("batch_get_transactions failed for {digest}: {status:?}")
        });

        let grpc_ulro = grpc_transaction
            .effects
            .map(|e| e.unchanged_loaded_runtime_objects)
            .unwrap_or_default();
        let expected_ulro: Vec<_> = indexed
            .unchanged_loaded_runtime_objects
            .iter()
            .map(Into::into)
            .collect();
        assert_eq!(
            grpc_ulro, expected_ulro,
            "unchanged_loaded_runtime_objects mismatch for transaction {digest}"
        );
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
        let summary = cp.summary.as_ref().unwrap();
        let contents = cp.contents.as_ref().unwrap();
        assert!(checkpoint_numbers.contains(&summary.sequence_number));
        assert!(summary.epoch == 0);

        let content_digests: Vec<_> = contents.iter().map(|ed| ed.transaction).collect();
        let expected: Vec<_> = tx_digests
            .iter()
            .zip(&tx_checkpoints)
            .filter(|(_, cp_num)| **cp_num == summary.sequence_number)
            .map(|(d, _)| *d)
            .collect();
        for d in &expected {
            assert!(
                content_digests.contains(d),
                "checkpoint {} should contain txn {}",
                summary.sequence_number,
                d,
            );
        }
    }

    // -- Checkpoint-by-digest reverse index --
    for cp in &checkpoints {
        let summary = cp.summary.as_ref().unwrap();
        let digest = summary.digest();
        let found = harness
            .bigtable_client()
            .get_checkpoint_by_digest(digest)
            .await?;
        assert!(found.is_some(), "checkpoint by digest should exist");
        assert_eq!(
            found.unwrap().summary.as_ref().unwrap().sequence_number,
            summary.sequence_number
        );
    }

    // -- Objects lookup --
    let mut object_keys: Vec<ObjectKey> = Vec::new();
    for tx_data in &transactions {
        for (obj_ref, _owner, _write_kind) in
            tx_data.effects.as_ref().unwrap().all_changed_objects()
        {
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

    // -- Package reader tests --
    // Find the package_id from the publish transaction's effects
    let publish_txns = harness
        .bigtable_client()
        .get_transactions(&[publish_digest])
        .await?;
    let publish_tx_data = publish_txns.first().context("publish tx not found")?;
    let created = publish_tx_data.effects.as_ref().unwrap().created();
    let (package_ref, _) = created
        .iter()
        .find(|(_, owner)| *owner == Owner::Immutable)
        .context("no immutable object (package) created")?;
    let package_id: ObjectID = package_ref.0;
    let package_version = package_ref.1.value();

    // For a newly published (non-upgrade) package, original_id == package_id
    let original_id = package_id;

    // get_package_original_ids: resolve package_id → original_id
    let orig_ids = harness
        .bigtable_client()
        .get_package_original_ids(&[package_id])
        .await?;
    assert_eq!(orig_ids.len(), 1);
    assert_eq!(orig_ids[0].0, package_id);
    assert_eq!(orig_ids[0].1, original_id);

    // get_packages_by_version: fetch by (original_id, version)
    let pkgs = harness
        .bigtable_client()
        .get_packages_by_version(&[(original_id, package_version)])
        .await?;
    assert_eq!(pkgs.len(), 1);
    assert_eq!(pkgs[0].package_id, package_id.to_vec());
    assert_eq!(pkgs[0].original_id, original_id.to_vec());
    assert_eq!(pkgs[0].package_version, package_version);
    assert!(!pkgs[0].is_system_package);

    // get_package_latest: latest version at or before max_checkpoint
    let latest_pkg = harness
        .bigtable_client()
        .get_package_latest(original_id, max_checkpoint)
        .await?
        .expect("package should exist");
    assert_eq!(latest_pkg.package_id, package_id.to_vec());
    assert_eq!(latest_pkg.package_version, package_version);

    // get_package_versions: paginate versions
    let versions = harness
        .bigtable_client()
        .get_package_versions(original_id, max_checkpoint, None, None, 10, false)
        .await?;
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].package_version, package_version);

    // get_packages_by_checkpoint_range: should include the published package
    let by_cp = harness
        .bigtable_client()
        .get_packages_by_checkpoint_range(None, None, 100, false)
        .await?;
    assert!(
        by_cp.iter().any(|p| p.package_id == package_id.to_vec()),
        "published package should appear in checkpoint range scan",
    );

    // get_system_packages: genesis system packages (0x1, 0x2, etc.)
    let sys_pkgs = harness
        .bigtable_client()
        .get_system_packages(max_checkpoint, None, 100)
        .await?;
    assert!(
        !sys_pkgs.is_empty(),
        "there should be system packages from genesis"
    );
    for pkg in &sys_pkgs {
        assert!(pkg.is_system_package);
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

    // -- Column-filtered checkpoint partial reads --
    // Fetch with only summary column — no signatures or contents.
    let partial = harness
        .bigtable_client()
        .get_checkpoints_filtered(&checkpoint_numbers, Some(&[checkpoints::col::SUMMARY]))
        .await?;
    assert_eq!(partial.len(), checkpoint_numbers.len());
    for cp in &partial {
        assert!(cp.summary.is_some(), "summary should be present");
        assert!(cp.signatures.is_none(), "signatures should be absent");
        assert!(cp.contents.is_none(), "contents should be absent");
    }

    // Fetch checkpoint by digest with only summary — verify two-step lookup works with filter.
    let first_cp = checkpoints.first().unwrap();
    let digest = first_cp.summary.as_ref().unwrap().digest();
    let partial = harness
        .bigtable_client()
        .get_checkpoint_by_digest_filtered(digest, Some(&[checkpoints::col::SUMMARY]))
        .await?
        .expect("checkpoint by digest should exist");
    assert!(partial.summary.is_some(), "summary should be present");
    assert!(partial.signatures.is_none(), "signatures should be absent");
    assert!(partial.contents.is_none(), "contents should be absent");

    // -- Column-filtered epoch partial reads --
    // Fetch epoch with only small scalar columns — no system_state.
    let partial = harness
        .bigtable_client()
        .get_epochs_filtered(
            &[0],
            Some(&[
                epochs::col::EPOCH,
                epochs::col::START_CHECKPOINT,
                epochs::col::PROTOCOL_VERSION,
            ]),
        )
        .await?;
    assert_eq!(partial.len(), 1);
    let ep = &partial[0];
    assert_eq!(ep.epoch, Some(0));
    assert!(
        ep.start_checkpoint.is_some(),
        "start_checkpoint should be present"
    );
    assert!(
        ep.protocol_version.is_some(),
        "protocol_version should be present"
    );
    assert!(ep.system_state.is_none(), "system_state should be absent");
    assert!(
        ep.reference_gas_price.is_none(),
        "reference_gas_price should be absent"
    );

    // Fetch latest epoch with column filter.
    let partial = harness
        .bigtable_client()
        .get_latest_epoch_filtered(Some(&[epochs::col::EPOCH, epochs::col::START_TIMESTAMP]))
        .await?
        .expect("latest epoch should exist");
    assert!(partial.epoch.is_some());
    assert!(
        partial.start_timestamp_ms.is_some(),
        "start_timestamp should be present"
    );
    assert!(
        partial.system_state.is_none(),
        "system_state should be absent"
    );
    assert!(
        partial.end_checkpoint.is_none(),
        "end_checkpoint should be absent"
    );

    Ok(())
}
