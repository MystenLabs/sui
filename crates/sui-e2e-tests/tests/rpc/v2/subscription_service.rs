// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::transfer_coin;
use prost::bytes::Bytes;
use std::path::PathBuf;
use sui_macros::sim_test;
use sui_rpc::Client;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::subscription_service_client::SubscriptionServiceClient;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tokio_stream::StreamExt;
use tonic::transport::Channel;

#[sim_test]
async fn subscribe_checkpoint() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let mut client = SubscriptionServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut request = SubscribeCheckpointsRequest::default();
    request.read_mask = Some(FieldMask::from_str("sequence_number"));

    let mut stream = client
        .subscribe_checkpoints(request)
        .await
        .unwrap()
        .into_inner();

    let mut count = 0;
    let mut last = None;
    while let Some(item) = stream.next().await {
        let checkpoint = item.unwrap();
        let cursor = checkpoint.cursor.unwrap();
        assert_eq!(
            cursor,
            checkpoint.checkpoint.unwrap().sequence_number.unwrap()
        );
        println!("checkpoint: {cursor}");

        if let Some(last) = last {
            assert_eq!(last, cursor - 1);
        }
        last = Some(cursor);

        // Subscribe for 50 checkponts to ensure the subscription system works
        count += 1;
        if count > 50 {
            break;
        }
    }

    assert!(count >= 50);
}

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// A cluster with ledger-history indexing on (so subscription cursors can be
/// replayed via the List APIs) and a tight watermark interval so sparse
/// subscriptions tick quickly.
async fn subscription_cluster() -> TestCluster {
    TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .with_rpc_config(sui_config::RpcConfig {
            enable_indexing: Some(true),
            subscription_watermark_interval: Some(2),
            ..Default::default()
        })
        .build()
        .await
}

async fn alpha_subscription_client(cluster: &TestCluster) -> SubscriptionServiceClient<Channel> {
    SubscriptionServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap()
}

fn sender_tx_filter(address: SuiAddress, negated: bool) -> v2::TransactionFilter {
    let mut sender = v2::SenderFilter::default();
    sender.address = Some(address.to_string());
    let mut literal = v2::TransactionLiteral::default();
    literal.predicate = Some(v2::transaction_literal::Predicate::Sender(sender));
    literal.negated = negated;
    let mut term = v2::TransactionTerm::default();
    term.literals = vec![literal];
    let mut filter = v2::TransactionFilter::default();
    filter.terms = vec![term];
    filter
}

fn emit_module_event_filter(module: &str) -> v2::EventFilter {
    let mut emit_module = v2::EmitModuleFilter::default();
    emit_module.module = Some(module.to_owned());
    let mut literal = v2::EventLiteral::default();
    literal.predicate = Some(v2::event_literal::Predicate::EmitModule(emit_module));
    let mut term = v2::EventTerm::default();
    term.literals = vec![literal];
    let mut filter = v2::EventFilter::default();
    filter.terms = vec![term];
    filter
}

async fn gas_object(cluster: &TestCluster, sender: SuiAddress) -> ObjectRef {
    cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .expect("sender should have a gas object")
}

/// Execute a PTB and wait for its checkpoint, so `tx.checkpoint` is
/// populated and the rpc index has committed it.
async fn execute_programmable(
    cluster: &TestCluster,
    sender: SuiAddress,
    builder: ProgrammableTransactionBuilder,
) -> ExecutedTransaction {
    let gas = gas_object(cluster, sender).await;
    let gas_price = cluster.wallet.get_reference_gas_price().await.unwrap();
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        gas_price,
    );
    let transaction = cluster.wallet.sign_transaction(&data).await;
    let mut client = Client::new(cluster.rpc_url().to_owned()).unwrap();
    super::execute_transaction(&mut client, &transaction).await
}

async fn submit_programmable(
    cluster: &TestCluster,
    sender: SuiAddress,
    builder: ProgrammableTransactionBuilder,
) -> String {
    let gas = gas_object(cluster, sender).await;
    let gas_price = cluster.wallet.get_reference_gas_price().await.unwrap();
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        gas_price,
    );
    let signed_transaction = cluster.wallet.sign_transaction(&data).await;
    let expected_digest = signed_transaction.digest().to_string();

    let mut transaction = v2::Transaction::default();
    transaction.bcs = Some(v2::Bcs::serialize(signed_transaction.transaction_data()).unwrap());
    let mut request = v2::ExecuteTransactionRequest::default();
    request.transaction = Some(transaction);
    request.signatures = signed_transaction
        .tx_signatures()
        .iter()
        .map(|signature| {
            let mut message = v2::UserSignature::default();
            message.bcs = Some(v2::Bcs::from(signature.as_ref().to_owned()));
            message
        })
        .collect();
    request.read_mask = Some(FieldMask::from_paths(["digest"]));

    let mut client =
        v2::transaction_execution_service_client::TransactionExecutionServiceClient::connect(
            cluster.rpc_url().to_owned(),
        )
        .await
        .unwrap();
    let response = client
        .execute_transaction(request)
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        response.transaction().digest.as_deref(),
        Some(expected_digest.as_str())
    );
    expected_digest
}

async fn transfer_self(cluster: &TestCluster, sender: SuiAddress) -> ExecutedTransaction {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender, None);
    execute_programmable(cluster, sender, builder).await
}

/// Fold a watermark into the non-decreasing checkpoint tracker, asserting
/// subscription watermarks never move backwards.
fn assert_checkpoint_monotone(last: &mut Option<u64>, watermark: &v2::Watermark) {
    if let Some(checkpoint) = watermark.checkpoint {
        if let Some(prev) = *last {
            assert!(
                checkpoint >= prev,
                "checkpoint watermark went backwards: {prev} -> {checkpoint}"
            );
        }
        *last = Some(checkpoint);
    }
}

fn payload_message_count(cluster: &TestCluster, item_type: &str) -> u64 {
    let metric_families = cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.prometheus_metrics_for_testing());
    metric_families
        .iter()
        .find(|family| family.name() == "subscription_payload_messages")
        .and_then(|family| {
            family.get_metric().iter().find(|metric| {
                metric
                    .get_label()
                    .iter()
                    .any(|label| label.name() == "type" && label.value() == item_type)
            })
        })
        .map(|metric| metric.counter.value() as u64)
        .unwrap_or(0)
}

#[sim_test]
async fn subscribe_transactions_sender_filter() {
    let cluster = subscription_cluster().await;
    let sender = cluster.get_address_0();

    let mut client = alpha_subscription_client(&cluster).await;
    let mut request = v2::SubscribeTransactionsRequest::default();
    request.read_mask = Some(FieldMask::from_paths([
        "digest",
        "transaction.digest",
        "effects.transaction_digest",
        "transaction_index",
    ]));
    request.filter = Some(sender_tx_filter(sender, false));
    let mut stream = client
        .subscribe_transactions(request)
        .await
        .unwrap()
        .into_inner();

    let tx = transfer_self(&cluster, sender).await;
    let execution_digest = tx.digest.as_deref().expect("executed tx has a digest");
    let execution_transaction_digest = tx
        .transaction
        .as_ref()
        .expect("executed tx has a transaction")
        .digest
        .as_deref()
        .expect("executed transaction has a digest");
    let execution_effects_digest = tx
        .effects
        .as_ref()
        .expect("executed tx has effects")
        .transaction_digest
        .as_deref()
        .expect("executed effects have a transaction digest");
    assert_eq!(execution_transaction_digest, execution_digest);
    assert_eq!(execution_effects_digest, execution_digest);
    let expected_digest = execution_digest.to_owned();

    let mut last_hi = None;
    let mut found = false;
    while let Some(frame) = stream.next().await {
        let frame = frame.unwrap();
        let watermark = frame.watermark.as_ref().expect("frame watermark");
        assert!(
            watermark.cursor.is_some(),
            "frame watermark carries a cursor"
        );
        assert_checkpoint_monotone(&mut last_hi, watermark);
        let Some(transaction) = frame.transaction.as_ref() else {
            continue;
        };
        assert!(transaction.transaction_index.is_some());
        let digest = transaction
            .digest
            .as_deref()
            .expect("digest requested in read mask");
        let nested_transaction_digest = transaction
            .transaction
            .as_ref()
            .expect("transaction requested in read mask")
            .digest
            .as_deref()
            .expect("transaction digest requested in read mask");
        let effects_transaction_digest = transaction
            .effects
            .as_ref()
            .expect("effects requested in read mask")
            .transaction_digest
            .as_deref()
            .expect("effects transaction digest requested in read mask");
        assert_eq!(
            digest,
            expected_digest.as_str(),
            "only sender-filtered items"
        );
        assert_eq!(nested_transaction_digest, expected_digest.as_str());
        assert_eq!(effects_transaction_digest, expected_digest.as_str());
        found = true;
        break;
    }
    assert!(found, "transfer should arrive on the filtered stream");
}

#[sim_test]
async fn subscribe_transactions_unfiltered() {
    let cluster = subscription_cluster().await;
    let sender = cluster.get_address_0();

    let mut client = alpha_subscription_client(&cluster).await;
    let mut request = v2::SubscribeTransactionsRequest::default();
    request.read_mask = Some(FieldMask::from_paths([
        "digest",
        "transaction_index",
        "checkpoint",
    ]));
    // No filter: the stream must deliver every transaction, exercising the
    // `AllTransactions` synthesis and its `0..tx_count` index expansion.
    let mut stream = client
        .subscribe_transactions(request)
        .await
        .unwrap()
        .into_inner();

    let tx = transfer_self(&cluster, sender).await;
    let expected_digest = tx.digest().to_owned();
    let tx_checkpoint = tx.checkpoint.expect("executed tx has a checkpoint");

    // Collect every transaction the unfiltered stream reports for the
    // transfer's checkpoint, stopping once a later checkpoint or a boundary
    // watermark proves it complete.
    let mut indices = Vec::new();
    let mut digests = Vec::new();
    let mut last_hi = None;
    loop {
        let frame = stream.next().await.expect("stream open").unwrap();
        let watermark = frame.watermark.as_ref().expect("frame watermark");
        assert!(
            watermark.cursor.is_some(),
            "frame watermark carries a cursor"
        );
        assert_checkpoint_monotone(&mut last_hi, watermark);
        if let Some(transaction) = frame.transaction.as_ref() {
            let checkpoint = transaction.checkpoint.expect("checkpoint in read mask");
            if checkpoint > tx_checkpoint {
                break;
            }
            if checkpoint < tx_checkpoint {
                continue;
            }
            indices.push(transaction.transaction_index.expect("index in read mask"));
            digests.push(transaction.digest.clone().expect("digest in read mask"));
        } else if watermark.checkpoint.is_some_and(|cp| cp >= tx_checkpoint) {
            break;
        }
    }

    assert!(
        digests.contains(&expected_digest),
        "unfiltered stream must deliver the sender's own transfer: {digests:?}"
    );
    assert!(
        digests.iter().any(|digest| *digest != expected_digest),
        "unfiltered stream also delivers transactions this sender never issued: {digests:?}"
    );
    indices.sort_unstable();
    assert_eq!(
        indices,
        (0..indices.len() as u64).collect::<Vec<_>>(),
        "AllTransactions expands to a gap-free 0..n index range: {indices:?}"
    );
}

#[sim_test]
async fn subscribe_checkpoints_filtered_and_unfiltered() {
    let cluster = subscription_cluster().await;
    let sender = cluster.get_address_0();

    let mut client = alpha_subscription_client(&cluster).await;

    let mut request = v2::SubscribeCheckpointsRequest::default();
    request.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    let mut unfiltered = client
        .subscribe_checkpoints(request)
        .await
        .unwrap()
        .into_inner();

    let mut request = v2::SubscribeCheckpointsRequest::default();
    request.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    request.filter = Some(sender_tx_filter(sender, false));
    let mut filtered = client
        .subscribe_checkpoints(request)
        .await
        .unwrap()
        .into_inner();

    let tx = transfer_self(&cluster, sender).await;
    let tx_checkpoint = tx.checkpoint.expect("executed tx has a checkpoint");

    // Unfiltered: consecutive checkpoint items, each carrying its own
    // sequence number as the covered cursor.
    let mut last_seq = None;
    let mut items = 0;
    while items < 5 {
        let frame = unfiltered.next().await.expect("stream open").unwrap();
        let cursor = frame.cursor.expect("cursor present on every frame");
        let checkpoint = frame
            .checkpoint
            .as_ref()
            .expect("unfiltered checkpoint subscription only emits items");
        let sequence_number = checkpoint
            .sequence_number
            .expect("sequence_number requested in read mask");
        assert_eq!(cursor, sequence_number, "an emitted checkpoint is complete");
        if let Some(last) = last_seq {
            assert_eq!(sequence_number, last + 1, "consecutive checkpoints");
        }
        last_seq = Some(sequence_number);
        items += 1;
    }

    // Filtered: cursors advance monotonically across progress-only and item
    // frames, and the first matching checkpoint is the transfer's.
    let mut last_cursor = None;
    loop {
        let frame = filtered.next().await.expect("stream open").unwrap();
        let cursor = frame.cursor.expect("cursor present on every frame");
        if let Some(previous) = last_cursor {
            assert!(
                cursor >= previous,
                "checkpoint cursor went backwards: {previous} -> {cursor}"
            );
        }
        last_cursor = Some(cursor);
        let Some(checkpoint) = frame.checkpoint.as_ref() else {
            continue;
        };
        let sequence_number = checkpoint
            .sequence_number
            .expect("sequence_number requested in read mask");
        assert_eq!(cursor, sequence_number, "item cursor matches checkpoint");
        assert_eq!(sequence_number, tx_checkpoint);
        break;
    }
}

#[sim_test]
async fn subscribe_events_filtered() {
    let cluster = subscription_cluster().await;
    let sender = cluster.get_address_0();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rpc/data/ledger_history/event/emit_test_event");
    let (pkg, _) = super::publish_package(&cluster, sender, path).await;

    let mut client = alpha_subscription_client(&cluster).await;
    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));
    let mut request = v2::SubscribeEventsRequest::default();
    request.read_mask = Some(FieldMask::from_paths([
        "checkpoint",
        "transaction_digest",
        "transaction_index",
        "event_index",
        "event_type",
    ]));
    request.filter = Some(emit_module_event_filter(&module));
    let mut stream = client.subscribe_events(request).await.unwrap().into_inner();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.programmable_move_call(
        pkg,
        move_core_types::identifier::Identifier::new("emit_test_event").unwrap(),
        move_core_types::identifier::Identifier::new("emit_test_event").unwrap(),
        vec![],
        vec![],
    );
    let tx = execute_programmable(&cluster, sender, builder).await;
    let expected_digest = tx.digest().to_owned();
    let tx_checkpoint = tx.checkpoint.expect("executed tx has a checkpoint");

    loop {
        let frame = stream.next().await.expect("stream open").unwrap();
        let watermark = frame.watermark.as_ref().expect("frame watermark");
        assert!(
            watermark.cursor.is_some(),
            "frame watermark carries a cursor"
        );
        let Some(proto_event) = frame.event.as_ref() else {
            continue;
        };
        assert_eq!(
            proto_event.transaction_digest.as_deref(),
            Some(&*expected_digest)
        );
        assert_eq!(proto_event.checkpoint, Some(tx_checkpoint));
        assert_eq!(proto_event.event_index, Some(0));
        assert!(proto_event.transaction_index.is_some());
        let event_type = proto_event
            .event_type
            .as_deref()
            .expect("event_type requested in read mask");
        assert!(
            event_type.contains("emit_test_event::TestEvent"),
            "unexpected event type {event_type}"
        );
        break;
    }
}

#[sim_test]
async fn subscribe_events_unfiltered() {
    let cluster = subscription_cluster().await;
    let sender = cluster.get_address_0();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rpc/data/ledger_history/event/emit_test_event");
    let (pkg, _) = super::publish_package(&cluster, sender, path).await;

    let mut client = alpha_subscription_client(&cluster).await;
    let mut request = v2::SubscribeEventsRequest::default();
    request.read_mask = Some(FieldMask::from_paths([
        "checkpoint",
        "transaction_digest",
        "transaction_index",
        "event_index",
    ]));
    // No filter: the stream must deliver every event, exercising the
    // `AllEvents` synthesis and its per-transaction index expansion.
    let mut stream = client.subscribe_events(request).await.unwrap().into_inner();

    // One transaction emitting two events, so `AllEvents` must expand to both.
    let mut builder = ProgrammableTransactionBuilder::new();
    for _ in 0..2 {
        builder.programmable_move_call(
            pkg,
            move_core_types::identifier::Identifier::new("emit_test_event").unwrap(),
            move_core_types::identifier::Identifier::new("emit_test_event").unwrap(),
            vec![],
            vec![],
        );
    }
    let tx = execute_programmable(&cluster, sender, builder).await;
    let expected_digest = tx.digest().to_owned();
    let tx_checkpoint = tx.checkpoint.expect("executed tx has a checkpoint");

    // Collect our transaction's events from the emitting checkpoint. Any other
    // transactions' events are ignored: the point is that the unfiltered
    // stream expands our tx's events, in order, with no filter registered.
    let mut event_indices = Vec::new();
    loop {
        let frame = stream.next().await.expect("stream open").unwrap();
        let watermark = frame.watermark.as_ref().expect("frame watermark");
        assert!(
            watermark.cursor.is_some(),
            "frame watermark carries a cursor"
        );
        if let Some(event) = frame.event.as_ref() {
            let checkpoint = event.checkpoint.expect("checkpoint in read mask");
            if checkpoint > tx_checkpoint {
                break;
            }
            if checkpoint < tx_checkpoint {
                continue;
            }
            if event.transaction_digest.as_deref() == Some(&*expected_digest) {
                assert!(event.transaction_index.is_some(), "tx index in read mask");
                event_indices.push(event.event_index.expect("event index in read mask"));
            }
        } else if watermark.checkpoint.is_some_and(|cp| cp >= tx_checkpoint) {
            break;
        }
    }

    assert_eq!(
        event_indices,
        vec![0, 1],
        "AllEvents delivers every event of the emitting transaction, in order: {event_indices:?}"
    );
}

#[sim_test]
async fn subscribe_watermark_ticks_on_sparse_filter() {
    let cluster = subscription_cluster().await;

    // A sender that never transacts: after the initial progress-only frame,
    // the stream must still make progress via interval ticks.
    let unused = SuiAddress::random_for_testing_only();
    let mut client = alpha_subscription_client(&cluster).await;
    let mut request = v2::SubscribeTransactionsRequest::default();
    request.read_mask = Some(FieldMask::from_paths(["digest"]));
    request.filter = Some(sender_tx_filter(unused, false));
    let mut stream = client
        .subscribe_transactions(request)
        .await
        .unwrap()
        .into_inner();

    let start = stream.next().await.expect("stream open").unwrap();
    assert!(
        start.transaction.is_none(),
        "filtered subscription starts with a progress-only frame"
    );
    let start_watermark = start.watermark.expect("start frame watermark");
    assert!(
        start_watermark.cursor.is_some(),
        "start frame carries a resume cursor"
    );
    assert_eq!(
        start_watermark.checkpoint, None,
        "start frame has not fully covered a checkpoint in the subscription interval"
    );

    let mut ticks = Vec::new();
    while ticks.len() < 3 {
        let frame = stream.next().await.expect("stream open").unwrap();
        assert!(
            frame.transaction.is_none(),
            "no item can match an unused sender: {:?}",
            frame.transaction
        );
        let watermark = frame.watermark.expect("frame watermark");
        assert!(watermark.cursor.is_some(), "tick carries a resume cursor");
        let checkpoint = watermark
            .checkpoint
            .expect("ascending stream sets checkpoint");
        ticks.push(checkpoint);
    }
    assert!(
        ticks.windows(2).all(|pair| pair[0] < pair[1]),
        "tick boundaries must strictly increase: {ticks:?}"
    );
}

#[sim_test]
async fn subscription_cursor_backfills_via_list() {
    let cluster = subscription_cluster().await;
    let sender = cluster.get_address_0();

    let mut client = alpha_subscription_client(&cluster).await;
    let mut request = v2::SubscribeTransactionsRequest::default();
    request.read_mask = Some(FieldMask::from_paths(["digest"]));
    request.filter = Some(sender_tx_filter(sender, false));
    let mut stream = client
        .subscribe_transactions(request)
        .await
        .unwrap()
        .into_inner();

    let tx1 = transfer_self(&cluster, sender).await;
    let digest1 = tx1.digest().to_owned();

    // Take transfer #1's item watermark as the client's resume point, then
    // drop the subscription.
    let mut resume_cursor: Option<Bytes> = None;
    while resume_cursor.is_none() {
        let frame = stream.next().await.expect("stream open").unwrap();
        let watermark = frame.watermark.expect("frame watermark");
        assert!(
            watermark.cursor.is_some(),
            "frame watermark carries a cursor"
        );
        let Some(transaction) = frame.transaction.as_ref() else {
            continue;
        };
        let digest = transaction
            .digest
            .as_deref()
            .expect("digest requested in read mask");
        assert_eq!(digest, digest1);
        resume_cursor = watermark.cursor;
    }
    drop(stream);

    // Transfer #2 happens while unsubscribed; replay the gap via List.
    let tx2 = transfer_self(&cluster, sender).await;
    let digest2 = tx2.digest().to_owned();

    let mut ledger = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let mut options = v2::QueryOptions::default();
    options.limit = Some(100);
    options.after = resume_cursor;
    let mut request = v2::ListTransactionsRequest::default();
    request.read_mask = Some(FieldMask::from_paths(["digest"]));
    request.filter = Some(sender_tx_filter(sender, false));
    request.options = Some(options);
    let mut list_stream = ledger
        .list_transactions(request)
        .await
        .unwrap()
        .into_inner();

    let mut digests = Vec::new();
    while let Some(response) = list_stream.message().await.unwrap() {
        if let Some(digest) = response.transaction.and_then(|tx| tx.digest) {
            digests.push(digest);
        }
    }
    assert!(
        digests.contains(&digest2),
        "backfill must return the missed transfer: {digests:?}"
    );
    assert!(
        !digests.contains(&digest1),
        "backfill must resume past the already-delivered transfer: {digests:?}"
    );
}

#[sim_test]
async fn subscribe_transactions_unanchored_negation() {
    let cluster = subscription_cluster().await;
    let sender_0 = cluster.get_address_0();
    let sender_1 = cluster.get_address_1();

    // NOT sender=address_1: matches every transaction (including system
    // transactions) except address_1's.
    let mut client = alpha_subscription_client(&cluster).await;
    let mut request = v2::SubscribeTransactionsRequest::default();
    request.read_mask = Some(FieldMask::from_paths(["digest"]));
    request.filter = Some(sender_tx_filter(sender_1, true));
    let mut stream = client
        .subscribe_transactions(request)
        .await
        .unwrap()
        .into_inner();

    let tx0 = transfer_self(&cluster, sender_0).await;
    let digest0 = tx0.digest().to_owned();
    let tx1 = transfer_self(&cluster, sender_1).await;
    let digest1 = tx1.digest().to_owned();
    // Once the watermark passes tx1's checkpoint, every matching item of
    // that checkpoint has been delivered.
    let bound = tx1.checkpoint.expect("executed tx has a checkpoint");

    let mut digests = Vec::new();
    loop {
        let frame = stream.next().await.expect("stream open").unwrap();
        let watermark = frame.watermark.expect("frame watermark");
        if let Some(transaction) = frame.transaction.as_ref() {
            let digest = transaction
                .digest
                .as_deref()
                .expect("digest requested in read mask");
            digests.push(digest.to_owned());
        }
        if watermark.checkpoint.is_some_and(|hi| hi >= bound) {
            break;
        }
    }
    assert!(
        digests.contains(&digest0),
        "address_0's transfer satisfies the negation: {digests:?}"
    );
    assert!(
        !digests.contains(&digest1),
        "address_1's transfer must be excluded: {digests:?}"
    );
}

#[sim_test]
async fn filtered_subscription_first_frame_is_progress_only() {
    let cluster = subscription_cluster().await;
    let sender = cluster.get_address_0();
    let filter = sender_tx_filter(sender, false);

    let mut client = alpha_subscription_client(&cluster).await;

    let mut transaction_request = v2::SubscribeTransactionsRequest::default();
    transaction_request.read_mask = Some(FieldMask::from_paths(["digest"]));
    transaction_request.filter = Some(filter.clone());
    let mut transactions = client
        .subscribe_transactions(transaction_request)
        .await
        .unwrap()
        .into_inner();

    let mut checkpoint_request = v2::SubscribeCheckpointsRequest::default();
    checkpoint_request.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    checkpoint_request.filter = Some(filter);
    let mut checkpoints = client
        .subscribe_checkpoints(checkpoint_request)
        .await
        .unwrap()
        .into_inner();

    let transaction_start = transactions
        .next()
        .await
        .expect("transaction stream open")
        .unwrap();
    assert!(
        transaction_start.transaction.is_none(),
        "filtered transaction subscription starts with a progress-only frame"
    );
    assert!(
        transaction_start
            .watermark
            .as_ref()
            .is_some_and(|watermark| watermark.cursor.is_some()),
        "transaction start frame carries a watermark and cursor"
    );
    assert_eq!(
        transaction_start
            .watermark
            .as_ref()
            .and_then(|watermark| watermark.checkpoint),
        None,
        "the start-frame watermark has not fully covered a checkpoint in the subscription interval",
    );

    let checkpoint_start = checkpoints
        .next()
        .await
        .expect("checkpoint stream open")
        .unwrap();
    assert!(
        checkpoint_start.checkpoint.is_none(),
        "filtered checkpoint subscription starts with a progress-only frame"
    );
    assert!(
        checkpoint_start.cursor.is_some(),
        "checkpoint start frame carries a cursor"
    );

    let tx = transfer_self(&cluster, sender).await;
    let expected_digest = tx.digest().to_owned();
    let expected_checkpoint = tx.checkpoint.expect("executed tx has a checkpoint");

    loop {
        let frame = transactions
            .next()
            .await
            .expect("transaction stream open")
            .unwrap();
        assert!(
            frame
                .watermark
                .as_ref()
                .is_some_and(|watermark| watermark.cursor.is_some()),
            "every transaction frame carries a watermark and cursor"
        );
        let Some(transaction) = frame.transaction.as_ref() else {
            continue;
        };
        assert_eq!(
            transaction.digest.as_deref(),
            Some(&*expected_digest),
            "matching transaction arrives after the start frame"
        );
        break;
    }

    loop {
        let frame = checkpoints
            .next()
            .await
            .expect("checkpoint stream open")
            .unwrap();
        let cursor = frame.cursor.expect("cursor present on every frame");
        let Some(checkpoint) = frame.checkpoint.as_ref() else {
            continue;
        };
        let sequence_number = checkpoint
            .sequence_number
            .expect("sequence_number requested in read mask");
        assert_eq!(cursor, sequence_number, "item cursor matches checkpoint");
        assert_eq!(
            sequence_number, expected_checkpoint,
            "matching checkpoint arrives after the start frame"
        );
        break;
    }
}

#[sim_test]
async fn subscribe_with_invalid_filter_is_rejected() {
    let cluster = subscription_cluster().await;

    // A present filter with zero terms is invalid, and is rejected on stream
    // open rather than surfacing as a stream error.
    let mut client = alpha_subscription_client(&cluster).await;
    let mut request = v2::SubscribeTransactionsRequest::default();
    request.filter = Some(v2::TransactionFilter::default());
    let status = client
        .subscribe_transactions(request)
        .await
        .expect_err("empty filter terms must be rejected");
    assert_eq!(status.code(), tonic::Code::InvalidArgument);
}

#[sim_test]
async fn subscription_payload_metric_records_emitted_items() {
    let cluster = subscription_cluster().await;
    let package_sender = cluster.get_address_0();
    let sender = cluster.get_address_2();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rpc/data/ledger_history/event/emit_test_event");
    let (pkg, _) = super::publish_package(&cluster, package_sender, path).await;
    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));

    let mut client = alpha_subscription_client(&cluster).await;
    let mut checkpoint_request = v2::SubscribeCheckpointsRequest::default();
    checkpoint_request.filter = Some(sender_tx_filter(sender, false));
    let mut checkpoints = client
        .subscribe_checkpoints(checkpoint_request)
        .await
        .unwrap()
        .into_inner();

    let mut transaction_request = v2::SubscribeTransactionsRequest::default();
    transaction_request.filter = Some(sender_tx_filter(sender, false));
    let mut transactions = client
        .subscribe_transactions(transaction_request)
        .await
        .unwrap()
        .into_inner();

    let mut event_request = v2::SubscribeEventsRequest::default();
    event_request.filter = Some(emit_module_event_filter(&module));
    let mut events = client
        .subscribe_events(event_request)
        .await
        .unwrap()
        .into_inner();

    let checkpoint_start = checkpoints
        .next()
        .await
        .expect("checkpoint stream open")
        .unwrap();
    assert!(checkpoint_start.checkpoint.is_none());

    let transaction_start = transactions
        .next()
        .await
        .expect("transaction stream open")
        .unwrap();
    assert!(transaction_start.transaction.is_none());

    let event_start = events.next().await.expect("event stream open").unwrap();
    assert!(event_start.event.is_none());

    let checkpoint_baseline = payload_message_count(&cluster, "checkpoint");
    let transaction_baseline = payload_message_count(&cluster, "transaction");
    let event_baseline = payload_message_count(&cluster, "event");

    let mut builder = ProgrammableTransactionBuilder::new();
    for _ in 0..2 {
        builder.programmable_move_call(
            pkg,
            move_core_types::identifier::Identifier::new("emit_test_event").unwrap(),
            move_core_types::identifier::Identifier::new("emit_test_event").unwrap(),
            vec![],
            vec![],
        );
    }
    let expected_digest = submit_programmable(&cluster, sender, builder).await;

    let checkpoint = loop {
        let frame = checkpoints
            .next()
            .await
            .expect("checkpoint stream open")
            .unwrap();
        if let Some(checkpoint) = frame.checkpoint {
            break checkpoint;
        }
    };
    assert!(checkpoint.sequence_number.is_some());

    let transaction = loop {
        let frame = transactions
            .next()
            .await
            .expect("transaction stream open")
            .unwrap();
        if let Some(transaction) = frame.transaction {
            break transaction;
        }
    };
    assert_eq!(transaction.digest.as_deref(), Some(&*expected_digest));

    let mut emitted_events = 0;
    while emitted_events < 2 {
        let frame = events.next().await.expect("event stream open").unwrap();
        if frame.event.is_some() {
            emitted_events += 1;
        }
    }

    let checkpoint_delta = payload_message_count(&cluster, "checkpoint") - checkpoint_baseline;
    let transaction_delta = payload_message_count(&cluster, "transaction") - transaction_baseline;
    let event_delta = payload_message_count(&cluster, "event") - event_baseline;
    assert_eq!(checkpoint_delta, 1);
    assert_eq!(transaction_delta, 1);
    assert_eq!(event_delta, 2);
}

#[sim_test]
async fn subscription_payload_metric_excludes_progress_frames() {
    let cluster = subscription_cluster().await;
    let mut client = alpha_subscription_client(&cluster).await;
    let mut request = v2::SubscribeCheckpointsRequest::default();
    request.filter = Some(sender_tx_filter(cluster.get_address_0(), false));
    let mut checkpoints = client
        .subscribe_checkpoints(request)
        .await
        .unwrap()
        .into_inner();
    let checkpoint_baseline = payload_message_count(&cluster, "checkpoint");

    let progress = checkpoints
        .next()
        .await
        .expect("checkpoint stream open")
        .unwrap();
    assert!(progress.checkpoint.is_none());
    assert_eq!(
        payload_message_count(&cluster, "checkpoint"),
        checkpoint_baseline
    );
}
