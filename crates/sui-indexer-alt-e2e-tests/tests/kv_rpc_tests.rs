// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use move_core_types::ident_str;
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::AffectedObjectFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::EmitModuleFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::EventAndFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::EventFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::EventTypeFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::ListEventsRequest;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::ListTransactionsRequest;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::MoveCallFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::RecipientFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::SenderFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::TransactionAndFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::TransactionFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::TransactionNotFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::TransactionOrFilter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::event_filter;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::list_service_client::ListServiceClient;
use sui_kv_rpc::proto::sui::rpc::kv::v2alpha::transaction_filter;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// Publish a Move package and return `(pkg_id, updated_gas_ref)`.
async fn publish_package(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &AccountKeyPair,
    gas: ObjectRef,
    path: PathBuf,
) -> (ObjectID, ObjectRef) {
    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            TestTransactionBuilder::new(sender, gas, cluster.reference_gas_price())
                .with_gas_budget(DEFAULT_GAS_BUDGET)
                .publish(path)
                .build(),
            vec![kp],
        ))
        .expect("publish failed");

    let pkg_id = fx
        .created()
        .into_iter()
        .find_map(|((id, v, _), owner)| {
            (v.value() == 1 && matches!(owner, Owner::Immutable)).then_some(id)
        })
        .expect("package id");

    let new_gas = fx
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| *id == gas.0)
        .map(|((id, version, digest), _)| (id, version, digest))
        .expect("gas mutated");

    (pkg_id, new_gas)
}

fn emit_test_event_pkg_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/event/emit_test_event")
}

fn generic_event_pkg_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/event/generic_event")
}

/// Execute a no-arg, no-type-arg Move call and return `(tx_digest, updated_gas_ref)`.
async fn call_move(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &AccountKeyPair,
    gas: ObjectRef,
    pkg: ObjectID,
    module: &str,
    function: &str,
) -> (sui_types::digests::TransactionDigest, ObjectRef) {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.programmable_move_call(
        pkg,
        move_core_types::identifier::Identifier::new(module).unwrap(),
        move_core_types::identifier::Identifier::new(function).unwrap(),
        vec![],
        vec![],
    );
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (fx, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("move call failed");
    assert!(err.is_none(), "move call failed: {err:?}");
    let digest = *fx.transaction_digest();
    let new_gas = fx
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| *id == gas.0)
        .map(|((id, version, digest), _)| (id, version, digest))
        .expect("gas mutated");
    (digest, new_gas)
}

#[tokio::test]
async fn test_json_read_mask() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // Publish the emit_test_event package so we can emit events.
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/event/emit_test_event");
    let (publish_fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            TestTransactionBuilder::new(sender, gas, cluster.reference_gas_price())
                .with_gas_budget(DEFAULT_GAS_BUDGET)
                .publish(path)
                .build(),
            vec![&kp],
        ))
        .expect("Failed to publish");

    let pkg_id = publish_fx
        .created()
        .into_iter()
        .find_map(|((id, v, _), owner)| {
            (v.value() == 1 && matches!(owner, Owner::Immutable)).then_some(id)
        })
        .expect("Failed to find package ID");

    // Get updated gas ref after publish.
    let gas = publish_fx
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| *id == gas.0)
        .map(|((id, version, digest), _)| (id, version, digest))
        .expect("gas object should be mutated");

    // Call emit_test_event to create a transaction with events.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.programmable_move_call(
        pkg_id,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_test_event").to_owned(),
        vec![],
        vec![],
    );
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (event_fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("emit_test_event failed");
    assert!(error.is_none(), "emit_test_event failed: {error:?}");
    let event_tx_digest = *event_fx.transaction_digest();

    cluster.create_checkpoint().await;

    let mut client = LedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // -- Object JSON: requested --
    {
        let object = client
            .get_object({
                let mut req = GetObjectRequest::default();
                req.object_id = Some(gas.0.to_canonical_string(true));
                req.read_mask = Some(FieldMask::from_paths(["json", "object_id"]));
                req
            })
            .await
            .unwrap()
            .into_inner()
            .object
            .expect("object should be present");

        // Coin<SUI> renders as a struct with `id` (UID as hex string) and `balance` (u64 as string).
        let json = object
            .json
            .expect("json should be populated for a Move object");
        let fields = match json.kind {
            Some(prost_types::value::Kind::StructValue(s)) => s.fields,
            other => panic!("expected struct value, got: {other:?}"),
        };
        assert_eq!(
            fields.len(),
            2,
            "Coin<SUI> should have exactly 2 fields (id, balance), got: {:?}",
            fields.keys().collect::<Vec<_>>()
        );
        assert!(fields.contains_key("id"), "missing 'id' field");
        assert!(fields.contains_key("balance"), "missing 'balance' field");

        // The id should be the object's hex address.
        let id_value = fields["id"].kind.as_ref().unwrap();
        match id_value {
            prost_types::value::Kind::StringValue(s) => {
                assert_eq!(s, &gas.0.to_canonical_string(true));
            }
            other => panic!("expected id to be a string, got: {other:?}"),
        }
    }

    // -- Object JSON: not requested --
    {
        let object = client
            .get_object({
                let mut req = GetObjectRequest::default();
                req.object_id = Some(gas.0.to_canonical_string(true));
                req.read_mask = Some(FieldMask::from_paths(["object_id", "version"]));
                req
            })
            .await
            .unwrap()
            .into_inner()
            .object
            .expect("object should be present");

        assert!(
            object.json.is_none(),
            "json should not be populated when not requested"
        );
    }

    // -- Transaction event JSON --
    {
        let tx = client
            .get_transaction({
                let mut req = GetTransactionRequest::default();
                req.digest = Some(event_tx_digest.to_string());
                req.read_mask = Some(FieldMask::from_paths([
                    "digest",
                    "events.events.json",
                    "events.events.event_type",
                ]));
                req
            })
            .await
            .unwrap()
            .into_inner()
            .transaction
            .expect("transaction should be present");

        assert_eq!(tx.digest(), event_tx_digest.to_string());

        let events = tx.events.expect("events should be present");
        assert_eq!(events.events.len(), 1, "expected exactly 1 event");

        let event = &events.events[0];
        assert!(
            event.event_type().contains("emit_test_event::TestEvent"),
            "unexpected event type: {}",
            event.event_type()
        );

        // The event JSON should be a struct with a single `value: 1` field.
        let json = event.json.as_ref().expect("event json should be populated");
        let fields = match &json.kind {
            Some(prost_types::value::Kind::StructValue(s)) => &s.fields,
            other => panic!("expected struct value, got: {other:?}"),
        };
        assert_eq!(fields.len(), 1, "TestEvent has one field");
        let value = fields["value"].kind.as_ref().unwrap();
        match value {
            prost_types::value::Kind::StringValue(s) => {
                assert_eq!(s, "1", "TestEvent.value should be 1");
            }
            other => panic!("expected value to be a string, got: {other:?}"),
        }
    }
}

#[tokio::test]
async fn test_list_transactions_unfiltered() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // Execute a transfer transaction.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender, None);
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("transfer failed");
    let tx_digest = *fx.transaction_digest();

    cluster.create_checkpoint().await;

    let mut client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.page_size = Some(100);

    let resp = client.list_transactions(req).await.unwrap().into_inner();

    // Should have at least the genesis tx + our transfer.
    assert!(
        resp.transactions.len() >= 2,
        "expected at least 2 transactions, got {}",
        resp.transactions.len()
    );

    // Our transaction should be in the results.
    let digests: Vec<_> = resp
        .transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.as_ref()))
        .collect();
    assert!(
        digests.contains(&&tx_digest.to_string()),
        "expected to find tx {tx_digest} in results"
    );

    // Results should be ordered by checkpoint then tx index.
    for w in resp.transactions.windows(2) {
        let (a_cp, a_idx) = (w[0].checkpoint(), w[0].transaction_index());
        let (b_cp, b_idx) = (w[1].checkpoint(), w[1].transaction_index());
        assert!(
            (a_cp, a_idx) <= (b_cp, b_idx),
            "results should be ordered: ({a_cp}, {a_idx}) > ({b_cp}, {b_idx})"
        );
    }
}

#[tokio::test]
async fn test_list_transactions_with_sender_filter() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // Execute a transfer from our sender.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender, None);
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("transfer failed");

    cluster.create_checkpoint().await;

    let mut client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Filter by our sender.
    let mut sender_filter = SenderFilter::default();
    sender_filter.address = Some(sender.to_string());
    let mut tx_filter = TransactionFilter::default();
    tx_filter.filter = Some(transaction_filter::Filter::Sender(sender_filter));
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_filter);
    req.page_size = Some(100);

    let resp = client.list_transactions(req).await.unwrap().into_inner();
    assert!(
        !resp.transactions.is_empty(),
        "expected at least 1 transaction from sender"
    );
}

#[tokio::test]
async fn test_list_transactions_pagination() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, mut gas) = cluster.funded_account(20 * DEFAULT_GAS_BUDGET).unwrap();

    // Execute several transactions to ensure pagination.
    for _ in 0..3 {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(sender, None);
        let data = TransactionData::new_programmable(
            sender,
            vec![gas],
            builder.finish(),
            DEFAULT_GAS_BUDGET,
            cluster.reference_gas_price(),
        );
        let (fx, _) = cluster
            .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
            .expect("transfer failed");
        gas = fx
            .mutated()
            .into_iter()
            .find(|((id, _, _), _)| *id == gas.0)
            .map(|((id, version, digest), _)| (id, version, digest))
            .expect("gas object should be mutated");
    }

    cluster.create_checkpoint().await;

    let mut client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // First page: page_size=2
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.page_size = Some(2);

    let page1 = client.list_transactions(req).await.unwrap().into_inner();
    assert_eq!(
        page1.transactions.len(),
        2,
        "first page should have 2 items"
    );
    assert!(
        page1.next_page_token.is_some(),
        "should have next page token"
    );

    // Second page using the token.
    let mut req2 = ListTransactionsRequest::default();
    req2.read_mask = Some(FieldMask::from_paths(["digest"]));
    req2.page_size = Some(2);
    req2.page_token = page1.next_page_token;

    let page2 = client.list_transactions(req2).await.unwrap().into_inner();
    assert!(
        !page2.transactions.is_empty(),
        "second page should have items"
    );

    // No overlap between pages.
    let page1_digests: Vec<_> = page1
        .transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
        .collect();
    let page2_digests: Vec<_> = page2
        .transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
        .collect();
    for d in &page2_digests {
        assert!(
            !page1_digests.contains(d),
            "page2 should not overlap with page1"
        );
    }
}

#[tokio::test]
async fn test_list_events_unfiltered() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // Publish the emit_test_event package.
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/event/emit_test_event");
    let (publish_fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            TestTransactionBuilder::new(sender, gas, cluster.reference_gas_price())
                .with_gas_budget(DEFAULT_GAS_BUDGET)
                .publish(path)
                .build(),
            vec![&kp],
        ))
        .expect("Failed to publish");

    let pkg_id = publish_fx
        .created()
        .into_iter()
        .find_map(|((id, v, _), owner)| {
            (v.value() == 1 && matches!(owner, Owner::Immutable)).then_some(id)
        })
        .expect("Failed to find package ID");

    let gas = publish_fx
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| *id == gas.0)
        .map(|((id, version, digest), _)| (id, version, digest))
        .expect("gas object should be mutated");

    // Call emit_test_event.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.programmable_move_call(
        pkg_id,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_test_event").to_owned(),
        vec![],
        vec![],
    );
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (event_fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("emit_test_event failed");
    assert!(error.is_none(), "emit_test_event failed: {error:?}");
    let event_tx_digest = *event_fx.transaction_digest();

    cluster.create_checkpoint().await;

    let mut client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.page_size = Some(100);

    let resp = client.list_events(req).await.unwrap().into_inner();

    // Should find at least our emitted event.
    assert!(!resp.events.is_empty(), "expected at least 1 event");

    let found = resp.events.iter().any(|e| {
        e.transaction_digest
            .as_ref()
            .is_some_and(|d| d == &event_tx_digest.to_string())
    });
    assert!(found, "expected to find event from tx {event_tx_digest}");

    // Verify event type contains our module.
    let our_event = resp
        .events
        .iter()
        .find(|e| {
            e.transaction_digest
                .as_ref()
                .is_some_and(|d| d == &event_tx_digest.to_string())
        })
        .unwrap();
    let event_type = our_event
        .event
        .as_ref()
        .and_then(|e| e.event_type.as_ref())
        .expect("event_type should be present");
    assert!(
        event_type.contains("emit_test_event::TestEvent"),
        "unexpected event type: {event_type}"
    );
}

#[tokio::test]
async fn test_list_events_with_emit_module_filter() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // Publish and emit event.
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/event/emit_test_event");
    let (publish_fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            TestTransactionBuilder::new(sender, gas, cluster.reference_gas_price())
                .with_gas_budget(DEFAULT_GAS_BUDGET)
                .publish(path)
                .build(),
            vec![&kp],
        ))
        .expect("Failed to publish");

    let pkg_id = publish_fx
        .created()
        .into_iter()
        .find_map(|((id, v, _), owner)| {
            (v.value() == 1 && matches!(owner, Owner::Immutable)).then_some(id)
        })
        .expect("Failed to find package ID");

    let gas = publish_fx
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| *id == gas.0)
        .map(|((id, version, digest), _)| (id, version, digest))
        .expect("gas object should be mutated");

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.programmable_move_call(
        pkg_id,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_test_event").to_owned(),
        vec![],
        vec![],
    );
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("emit_test_event failed");

    cluster.create_checkpoint().await;

    let mut client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Filter by emit_module matching our package.
    let mut emit_mod = sui_kv_rpc::proto::sui::rpc::kv::v2alpha::EmitModuleFilter::default();
    emit_mod.module = Some(format!(
        "{}::emit_test_event",
        pkg_id.to_canonical_string(true)
    ));
    let mut filter = EventFilter::default();
    filter.filter = Some(event_filter::Filter::EmitModule(emit_mod));

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(filter);
    req.page_size = Some(100);

    let resp = client.list_events(req).await.unwrap().into_inner();
    assert!(
        !resp.events.is_empty(),
        "expected at least 1 event matching emit_module filter"
    );

    for event_result in &resp.events {
        let event_type = event_result
            .event
            .as_ref()
            .and_then(|e| e.event_type.as_ref())
            .expect("event_type should be present");
        assert!(
            event_type.contains("emit_test_event"),
            "all events should be from emit_test_event module, got: {event_type}"
        );
    }
}

#[tokio::test]
async fn test_list_events_pagination() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // Publish and emit events from multiple transactions.
    let path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/event/emit_test_event");
    let (publish_fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            TestTransactionBuilder::new(sender, gas, cluster.reference_gas_price())
                .with_gas_budget(DEFAULT_GAS_BUDGET)
                .publish(path)
                .build(),
            vec![&kp],
        ))
        .expect("Failed to publish");

    let pkg_id = publish_fx
        .created()
        .into_iter()
        .find_map(|((id, v, _), owner)| {
            (v.value() == 1 && matches!(owner, Owner::Immutable)).then_some(id)
        })
        .expect("Failed to find package ID");

    let mut gas = publish_fx
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| *id == gas.0)
        .map(|((id, version, digest), _)| (id, version, digest))
        .expect("gas object should be mutated");

    // Emit events from 3 separate transactions.
    for _ in 0..3 {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.programmable_move_call(
            pkg_id,
            ident_str!("emit_test_event").to_owned(),
            ident_str!("emit_test_event").to_owned(),
            vec![],
            vec![],
        );
        let data = TransactionData::new_programmable(
            sender,
            vec![gas],
            builder.finish(),
            DEFAULT_GAS_BUDGET,
            cluster.reference_gas_price(),
        );
        let (fx, _) = cluster
            .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
            .expect("emit_test_event failed");
        gas = fx
            .mutated()
            .into_iter()
            .find(|((id, _, _), _)| *id == gas.0)
            .map(|((id, version, digest), _)| (id, version, digest))
            .expect("gas object should be mutated");
    }

    cluster.create_checkpoint().await;

    let mut client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Use emit_module filter to find only events from our package.
    let mut emit_mod = sui_kv_rpc::proto::sui::rpc::kv::v2alpha::EmitModuleFilter::default();
    emit_mod.module = Some(pkg_id.to_canonical_string(true));
    let mut ev_filter = EventFilter::default();
    ev_filter.filter = Some(event_filter::Filter::EmitModule(emit_mod));

    // Paginate with page_size=1.
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(ev_filter.clone());
    req.page_size = Some(1);

    let page1 = client.list_events(req).await.unwrap().into_inner();
    assert_eq!(page1.events.len(), 1, "first page should have 1 event");
    assert!(
        page1.next_page_token.is_some(),
        "should have next page token"
    );

    let mut req2 = ListEventsRequest::default();
    req2.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req2.filter = Some(ev_filter);
    req2.page_size = Some(1);
    req2.page_token = page1.next_page_token;

    let page2 = client.list_events(req2).await.unwrap().into_inner();
    assert_eq!(page2.events.len(), 1, "second page should have 1 event");

    // Events on page2 should be different from page1.
    let p1_cursor = &page1.events[0].cursor;
    let p2_cursor = &page2.events[0].cursor;
    assert_ne!(p1_cursor, p2_cursor, "pages should have different cursors");
}

// --- Helper filter builders ---

fn tx_sender(addr: SuiAddress) -> TransactionFilter {
    let mut s = SenderFilter::default();
    s.address = Some(addr.to_string());
    let mut f = TransactionFilter::default();
    f.filter = Some(transaction_filter::Filter::Sender(s));
    f
}

fn tx_move_call(path: &str) -> TransactionFilter {
    let mut mc = MoveCallFilter::default();
    mc.function = Some(path.to_string());
    let mut f = TransactionFilter::default();
    f.filter = Some(transaction_filter::Filter::MoveCall(mc));
    f
}

fn ev_sender(addr: SuiAddress) -> EventFilter {
    let mut s = SenderFilter::default();
    s.address = Some(addr.to_string());
    let mut f = EventFilter::default();
    f.filter = Some(event_filter::Filter::Sender(s));
    f
}

fn ev_emit_module(path: &str) -> EventFilter {
    let mut em = EmitModuleFilter::default();
    em.module = Some(path.to_string());
    let mut f = EventFilter::default();
    f.filter = Some(event_filter::Filter::EmitModule(em));
    f
}

fn ev_event_type(path: &str) -> EventFilter {
    let mut et = EventTypeFilter::default();
    et.r#type = Some(path.to_string());
    let mut f = EventFilter::default();
    f.filter = Some(event_filter::Filter::EventType(et));
    f
}

#[tokio::test]
async fn test_list_transactions_combinator_and() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender_a, kp_a, gas_a) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();
    let (sender_b, kp_b, gas_b) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    let (pkg, gas_a) = publish_package(
        &mut cluster,
        sender_a,
        &kp_a,
        gas_a,
        emit_test_event_pkg_path(),
    )
    .await;

    // (a) sender A + matching move call — should be the only match.
    let (digest_a_call, gas_a) = call_move(
        &mut cluster,
        sender_a,
        &kp_a,
        gas_a,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;

    // (b) sender A + transfer (no move call).
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender_a, None);
    let data = TransactionData::new_programmable(
        sender_a,
        vec![gas_a],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (fx_b, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp_a]))
        .expect("transfer failed");
    let digest_a_transfer = *fx_b.transaction_digest();

    // (c) sender B + matching move call.
    let (digest_b_call, _) = call_move(
        &mut cluster,
        sender_b,
        &kp_b,
        gas_b,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;

    cluster.create_checkpoint().await;

    let mut client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let move_call_path = format!(
        "{}::emit_test_event::emit_test_event",
        pkg.to_canonical_string(true)
    );
    let mut and = TransactionAndFilter::default();
    and.filters = vec![tx_sender(sender_a), tx_move_call(&move_call_path)];
    let mut filter = TransactionFilter::default();
    filter.filter = Some(transaction_filter::Filter::And(and));

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(filter);
    req.page_size = Some(100);

    let resp = client.list_transactions(req).await.unwrap().into_inner();
    let digests: Vec<String> = resp
        .transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
        .collect();

    assert!(
        digests.contains(&digest_a_call.to_string()),
        "expected A+call to match"
    );
    assert!(
        !digests.contains(&digest_a_transfer.to_string()),
        "A's transfer should not match (no move call)"
    );
    assert!(
        !digests.contains(&digest_b_call.to_string()),
        "B's call should not match (wrong sender)"
    );
}

#[tokio::test]
async fn test_list_transactions_combinator_or_not() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender_a, kp_a, gas_a) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();
    let (sender_b, kp_b, gas_b) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // One tx from A.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender_a, None);
    let data = TransactionData::new_programmable(
        sender_a,
        vec![gas_a],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (fx_a, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp_a]))
        .expect("A tx failed");
    let digest_a = *fx_a.transaction_digest();

    // One tx from B.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender_b, None);
    let data = TransactionData::new_programmable(
        sender_b,
        vec![gas_b],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (fx_b, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp_b]))
        .expect("B tx failed");
    let digest_b = *fx_b.transaction_digest();

    cluster.create_checkpoint().await;

    let mut client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Or(Sender(A), Not(Sender(B))) — matches A + everything-not-B.
    let mut not_b = TransactionNotFilter::default();
    not_b.filter = Some(Box::new(tx_sender(sender_b)));
    let mut not_wrapped = TransactionFilter::default();
    not_wrapped.filter = Some(transaction_filter::Filter::Not(Box::new(not_b)));

    let mut or = TransactionOrFilter::default();
    or.filters = vec![tx_sender(sender_a), not_wrapped];
    let mut filter = TransactionFilter::default();
    filter.filter = Some(transaction_filter::Filter::Or(or));

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(filter);
    req.page_size = Some(100);

    let resp = client.list_transactions(req).await.unwrap().into_inner();
    let digests: Vec<String> = resp
        .transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
        .collect();

    assert!(
        digests.contains(&digest_a.to_string()),
        "A's tx should match"
    );
    assert!(
        !digests.contains(&digest_b.to_string()),
        "B's tx should be excluded by Not(Sender(B))"
    );
}

#[tokio::test]
async fn test_list_transactions_recipient_and_affected_object() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender_a, kp_a, gas_a) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();
    let (sender_b, _kp_b, _gas_b) = cluster.funded_account(DEFAULT_GAS_BUDGET).unwrap();

    // A transfers a split coin to B.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender_b, Some(1_000_000));
    let data = TransactionData::new_programmable(
        sender_a,
        vec![gas_a],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp_a]))
        .expect("transfer failed");
    let digest = *fx.transaction_digest();

    // The freshly created coin going to B.
    let transferred_coin_id = fx
        .created()
        .into_iter()
        .find_map(|((id, _, _), owner)| {
            matches!(owner, Owner::AddressOwner(addr) if addr == sender_b).then_some(id)
        })
        .expect("transferred coin");

    cluster.create_checkpoint().await;

    let mut client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Recipient(B) should include the transfer.
    let mut r = RecipientFilter::default();
    r.address = Some(sender_b.to_string());
    let mut rf = TransactionFilter::default();
    rf.filter = Some(transaction_filter::Filter::Recipient(r));
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(rf);
    req.page_size = Some(100);
    let resp = client.list_transactions(req).await.unwrap().into_inner();
    assert!(
        resp.transactions.iter().any(|t| t
            .transaction
            .as_ref()
            .and_then(|tx| tx.digest.as_deref())
            == Some(&digest.to_string())),
        "recipient filter should include the transfer tx"
    );

    // AffectedObject(coin_id) should include the transfer.
    let mut ao = AffectedObjectFilter::default();
    ao.object_id = Some(transferred_coin_id.to_canonical_string(true));
    let mut af = TransactionFilter::default();
    af.filter = Some(transaction_filter::Filter::AffectedObject(ao));
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(af);
    req.page_size = Some(100);
    let resp = client.list_transactions(req).await.unwrap().into_inner();
    assert!(
        resp.transactions.iter().any(|t| t
            .transaction
            .as_ref()
            .and_then(|tx| tx.digest.as_deref())
            == Some(&digest.to_string())),
        "affected_object filter should include the transfer tx"
    );
}

#[tokio::test]
async fn test_list_events_event_type_cascading_and_generics() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    let (pkg, gas) =
        publish_package(&mut cluster, sender, &kp, gas, generic_event_pkg_path()).await;

    let (digest_u64, gas) = call_move(
        &mut cluster,
        sender,
        &kp,
        gas,
        pkg,
        "generic_event",
        "emit_u64",
    )
    .await;
    let (digest_addr, _) = call_move(
        &mut cluster,
        sender,
        &kp,
        gas,
        pkg,
        "generic_event",
        "emit_address",
    )
    .await;

    cluster.create_checkpoint().await;

    let client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let pkg_hex = pkg.to_canonical_string(true);

    let fetch = |filter: EventFilter| {
        let mut c = client.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["event_type"]));
            req.filter = Some(filter);
            req.page_size = Some(100);
            c.list_events(req).await.unwrap().into_inner()
        }
    };

    // Name level (no generics) — matches both instantiations.
    let name = format!("{pkg_hex}::generic_event::GenericEvent");
    let resp = fetch(ev_event_type(&name)).await;
    let digests: Vec<String> = resp
        .events
        .iter()
        .filter_map(|e| e.transaction_digest.clone())
        .collect();
    assert!(
        digests.contains(&digest_u64.to_string()) && digests.contains(&digest_addr.to_string()),
        "name-level filter should match both events, got {digests:?}"
    );

    // Fully instantiated — only the u64 variant.
    let u64_type = format!("{pkg_hex}::generic_event::GenericEvent<u64>");
    let resp = fetch(ev_event_type(&u64_type)).await;
    let digests: Vec<String> = resp
        .events
        .iter()
        .filter_map(|e| e.transaction_digest.clone())
        .collect();
    assert!(
        digests.contains(&digest_u64.to_string()) && !digests.contains(&digest_addr.to_string()),
        "<u64> filter should match only the u64 event, got {digests:?}"
    );

    // Module-level cascading — matches both.
    let module = format!("{pkg_hex}::generic_event");
    let resp = fetch(ev_event_type(&module)).await;
    let digests: Vec<String> = resp
        .events
        .iter()
        .filter_map(|e| e.transaction_digest.clone())
        .collect();
    assert!(
        digests.contains(&digest_u64.to_string()) && digests.contains(&digest_addr.to_string()),
        "module-level filter should match both events, got {digests:?}"
    );
}

#[tokio::test]
async fn test_list_events_combinator_and() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender_a, kp_a, gas_a) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();
    let (sender_b, kp_b, gas_b) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    let (pkg, gas_a) = publish_package(
        &mut cluster,
        sender_a,
        &kp_a,
        gas_a,
        emit_test_event_pkg_path(),
    )
    .await;

    let (digest_a, _) = call_move(
        &mut cluster,
        sender_a,
        &kp_a,
        gas_a,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;
    let (digest_b, _) = call_move(
        &mut cluster,
        sender_b,
        &kp_b,
        gas_b,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;

    cluster.create_checkpoint().await;

    let mut client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));
    let mut and = EventAndFilter::default();
    and.filters = vec![ev_sender(sender_a), ev_emit_module(&module)];
    let mut filter = EventFilter::default();
    filter.filter = Some(event_filter::Filter::And(and));

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(filter);
    req.page_size = Some(100);

    let resp = client.list_events(req).await.unwrap().into_inner();
    let digests: Vec<String> = resp
        .events
        .iter()
        .filter_map(|e| e.transaction_digest.clone())
        .collect();

    assert!(
        digests.contains(&digest_a.to_string()),
        "A's event should match"
    );
    assert!(
        !digests.contains(&digest_b.to_string()),
        "B's event should be excluded by And(Sender=A, …)"
    );
}

#[tokio::test]
async fn test_list_events_pagination_multi_event_tx() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    let (pkg, gas) =
        publish_package(&mut cluster, sender, &kp, gas, emit_test_event_pkg_path()).await;

    // tx1: emit_many(5)
    let mut builder = ProgrammableTransactionBuilder::new();
    let arg = builder.pure(5u64).unwrap();
    builder.programmable_move_call(
        pkg,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_many").to_owned(),
        vec![],
        vec![arg],
    );
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (fx1, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("emit_many(5) failed");
    assert!(err.is_none(), "emit_many(5): {err:?}");
    let gas = fx1
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| *id == gas.0)
        .map(|((id, version, digest), _)| (id, version, digest))
        .expect("gas mutated");

    // tx2: emit_many(3)
    let mut builder = ProgrammableTransactionBuilder::new();
    let arg = builder.pure(3u64).unwrap();
    builder.programmable_move_call(
        pkg,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_many").to_owned(),
        vec![],
        vec![arg],
    );
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (_fx2, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("emit_many(3) failed");
    assert!(err.is_none(), "emit_many(3): {err:?}");

    cluster.create_checkpoint().await;

    let client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));

    let page = |filter: EventFilter, page_token: Option<prost::bytes::Bytes>| {
        let mut c = client.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["event_type"]));
            req.filter = Some(filter);
            req.page_size = Some(3);
            req.page_token = page_token;
            c.list_events(req).await.unwrap().into_inner()
        }
    };

    let f = ev_emit_module(&module);
    let p1 = page(f.clone(), None).await;
    assert_eq!(p1.events.len(), 3, "page 1 size");
    assert!(p1.next_page_token.is_some(), "page 1 needs next token");

    let p2 = page(f.clone(), p1.next_page_token.clone()).await;
    assert_eq!(p2.events.len(), 3, "page 2 size");
    assert!(p2.next_page_token.is_some(), "page 2 needs next token");

    let p3 = page(f, p2.next_page_token.clone()).await;
    assert_eq!(p3.events.len(), 2, "page 3 size");

    // No duplicates across pages, ordered by cursor.
    let mut all_cursors: Vec<_> = p1
        .events
        .iter()
        .chain(p2.events.iter())
        .chain(p3.events.iter())
        .map(|e| e.cursor.clone())
        .collect();
    let total = all_cursors.len();
    all_cursors.sort();
    all_cursors.dedup();
    assert_eq!(
        all_cursors.len(),
        total,
        "no duplicate cursors across pages"
    );
    assert_eq!(total, 8, "8 total events");
}

#[tokio::test]
async fn test_list_filter_edge_cases() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // One trivial tx to have something indexed.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender, None);
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("tx failed");
    cluster.create_checkpoint().await;

    let mut client = ListServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Checkpoint range beyond what's indexed → empty, no next token.
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(9999);
    req.page_size = Some(10);
    let resp = client.list_transactions(req).await.unwrap().into_inner();
    assert!(resp.transactions.is_empty(), "no txs beyond indexed range");
    assert!(
        resp.next_page_token.is_none(),
        "terminal page should have no token"
    );

    // Filter that matches nothing → empty, no next token.
    let never_sender: SuiAddress =
        "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
            .parse()
            .unwrap();
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_sender(never_sender));
    req.page_size = Some(10);
    let resp = client.list_transactions(req).await.unwrap().into_inner();
    assert!(resp.transactions.is_empty(), "no-match filter");
    assert!(
        resp.next_page_token.is_none(),
        "terminal page should have no token"
    );

    // Malformed MoveCall path (too many `::` parts) → InvalidArgument.
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_move_call("0x1::a::b::c"));
    req.page_size = Some(10);
    let err = client
        .list_transactions(req)
        .await
        .expect_err("should be InvalidArgument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // Malformed EventType (generics without a name) → InvalidArgument.
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(ev_event_type("0x1<u64>"));
    req.page_size = Some(10);
    let err = client
        .list_events(req)
        .await
        .expect_err("should be InvalidArgument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}
