// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use move_core_types::ident_str;
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2alpha::AffectedAddressFilter;
use sui_rpc::proto::sui::rpc::v2alpha::AffectedObjectFilter;
use sui_rpc::proto::sui::rpc::v2alpha::CheckpointItem;
use sui_rpc::proto::sui::rpc::v2alpha::EmitModuleFilter;
use sui_rpc::proto::sui::rpc::v2alpha::EventFilter;
use sui_rpc::proto::sui::rpc::v2alpha::EventItem;
use sui_rpc::proto::sui::rpc::v2alpha::EventLiteral;
use sui_rpc::proto::sui::rpc::v2alpha::EventPredicate;
use sui_rpc::proto::sui::rpc::v2alpha::EventStreamHeadFilter;
use sui_rpc::proto::sui::rpc::v2alpha::EventTerm;
use sui_rpc::proto::sui::rpc::v2alpha::EventTypeFilter;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::MoveCallFilter;
use sui_rpc::proto::sui::rpc::v2alpha::Ordering;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::QueryOptions;
use sui_rpc::proto::sui::rpc::v2alpha::SenderFilter;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionFilter;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionItem;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionLiteral;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionPredicate;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionTerm;
use sui_rpc::proto::sui::rpc::v2alpha::event_literal;
use sui_rpc::proto::sui::rpc::v2alpha::event_predicate;
use sui_rpc::proto::sui::rpc::v2alpha::ledger_service_client::LedgerServiceClient as KvLedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2alpha::list_checkpoints_response;
use sui_rpc::proto::sui::rpc::v2alpha::list_events_response;
use sui_rpc::proto::sui::rpc::v2alpha::list_transactions_response;
use sui_rpc::proto::sui::rpc::v2alpha::transaction_literal;
use sui_rpc::proto::sui::rpc::v2alpha::transaction_predicate;
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
use tonic::transport::Channel;

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;
const DEFAULT_CHECKPOINT_RANGE_END: u64 = 3_000_000;

struct TransactionsResult {
    transactions: Vec<TransactionItem>,
    end: bool,
    end_cursor: Option<prost::bytes::Bytes>,
    end_reason: Option<QueryEndReason>,
}

struct EventsResult {
    events: Vec<EventItem>,
    end: bool,
    end_cursor: Option<prost::bytes::Bytes>,
    end_reason: Option<QueryEndReason>,
}

struct CheckpointsResult {
    checkpoints: Vec<CheckpointItem>,
    end: bool,
    end_cursor: Option<prost::bytes::Bytes>,
    end_reason: Option<QueryEndReason>,
}

fn query_options(limit_items: u32) -> QueryOptions {
    let mut options = QueryOptions::default();
    options.limit_items = Some(limit_items);
    options
}

fn query_options_after(limit_items: u32, after: prost::bytes::Bytes) -> QueryOptions {
    let mut options = query_options(limit_items);
    options.after = Some(after);
    options
}

fn query_options_maybe_after(limit_items: u32, after: Option<prost::bytes::Bytes>) -> QueryOptions {
    let mut options = query_options(limit_items);
    options.after = after;
    options
}

fn query_options_descending_before(limit_items: u32, before: prost::bytes::Bytes) -> QueryOptions {
    let mut options = query_options(limit_items);
    options.before = Some(before);
    options.ordering = Ordering::Descending as i32;
    options
}

fn query_options_descending_maybe_before(
    limit_items: u32,
    before: Option<prost::bytes::Bytes>,
) -> QueryOptions {
    let mut options = query_options_descending(limit_items);
    options.before = before;
    options
}

fn query_options_descending(limit_items: u32) -> QueryOptions {
    let mut options = query_options(limit_items);
    options.ordering = Ordering::Descending as i32;
    options
}

fn query_options_between(
    limit_items: u32,
    after: prost::bytes::Bytes,
    before: prost::bytes::Bytes,
) -> QueryOptions {
    let mut options = query_options(limit_items);
    options.after = Some(after);
    options.before = Some(before);
    options
}

fn query_options_between_descending(
    limit_items: u32,
    after: prost::bytes::Bytes,
    before: prost::bytes::Bytes,
) -> QueryOptions {
    let mut options = query_options_between(limit_items, after, before);
    options.ordering = Ordering::Descending as i32;
    options
}

fn first_transaction_cursor(result: &TransactionsResult, message: &str) -> prost::bytes::Bytes {
    result
        .transactions
        .first()
        .and_then(|item| item.cursor.clone())
        .expect(message)
}

fn last_transaction_cursor(result: &TransactionsResult, message: &str) -> prost::bytes::Bytes {
    result
        .transactions
        .last()
        .and_then(|item| item.cursor.clone())
        .expect(message)
}

fn transaction_end_cursor(result: &TransactionsResult, message: &str) -> prost::bytes::Bytes {
    result.end_cursor.clone().expect(message)
}

fn first_event_cursor(result: &EventsResult, message: &str) -> prost::bytes::Bytes {
    result
        .events
        .first()
        .and_then(|item| item.cursor.clone())
        .expect(message)
}

fn last_event_cursor(result: &EventsResult, message: &str) -> prost::bytes::Bytes {
    result
        .events
        .last()
        .and_then(|item| item.cursor.clone())
        .expect(message)
}

fn event_end_cursor(result: &EventsResult, message: &str) -> prost::bytes::Bytes {
    result.end_cursor.clone().expect(message)
}

fn first_checkpoint_cursor(result: &CheckpointsResult, message: &str) -> prost::bytes::Bytes {
    result
        .checkpoints
        .first()
        .and_then(|item| item.cursor.clone())
        .expect(message)
}

fn last_checkpoint_cursor(result: &CheckpointsResult, message: &str) -> prost::bytes::Bytes {
    result
        .checkpoints
        .last()
        .and_then(|item| item.cursor.clone())
        .expect(message)
}

fn checkpoint_end_cursor(result: &CheckpointsResult, message: &str) -> prost::bytes::Bytes {
    result.end_cursor.clone().expect(message)
}

fn assert_item_limit_end(end: bool, reason: Option<QueryEndReason>) {
    assert!(end, "item-limit response should include end frame");
    assert_eq!(reason, Some(QueryEndReason::ItemLimit));
}

fn assert_transaction_cursors(result: &TransactionsResult) {
    for item in &result.transactions {
        assert!(item.cursor.is_some(), "transaction item should have cursor");
    }
}

fn assert_event_cursors(result: &EventsResult) {
    for item in &result.events {
        assert!(item.cursor.is_some(), "event item should have cursor");
    }
}

fn assert_checkpoint_cursors(result: &CheckpointsResult) {
    for item in &result.checkpoints {
        assert!(item.cursor.is_some(), "checkpoint item should have cursor");
    }
}

fn checkpoint_sequence(response: &CheckpointItem) -> u64 {
    response
        .checkpoint
        .as_ref()
        .and_then(|checkpoint| checkpoint.sequence_number)
        .expect("checkpoint sequence number should be populated")
}

async fn list_transactions_result(
    client: &mut KvLedgerServiceClient<Channel>,
    request: ListTransactionsRequest,
) -> TransactionsResult {
    let mut stream = client
        .list_transactions(request)
        .await
        .unwrap()
        .into_inner();
    let mut transactions = Vec::new();
    let mut end = false;
    let mut end_cursor = None;
    let mut end_reason = None;
    while let Some(response) = stream.message().await.unwrap() {
        match response.response.expect("list_transactions response frame") {
            list_transactions_response::Response::Item(item) => {
                assert!(!end, "item frame after end");
                transactions.push(item);
            }
            list_transactions_response::Response::End(end_frame) => {
                assert!(!end, "duplicate end frame");
                end = true;
                end_cursor = end_frame.cursor.clone();
                assert!(
                    end_cursor.is_some(),
                    "list_transactions end frame should include cursor"
                );
                end_reason = Some(
                    QueryEndReason::try_from(end_frame.reason)
                        .expect("valid list_transactions end reason"),
                );
            }
            other => panic!("unexpected list_transactions response frame: {other:?}"),
        }
    }
    TransactionsResult {
        transactions,
        end,
        end_cursor,
        end_reason,
    }
}

async fn list_events_result(
    client: &mut KvLedgerServiceClient<Channel>,
    request: ListEventsRequest,
) -> EventsResult {
    let mut stream = client.list_events(request).await.unwrap().into_inner();
    let mut events = Vec::new();
    let mut end = false;
    let mut end_cursor = None;
    let mut end_reason = None;
    while let Some(response) = stream.message().await.unwrap() {
        match response.response.expect("list_events response frame") {
            list_events_response::Response::Item(item) => {
                assert!(!end, "item frame after end");
                events.push(item);
            }
            list_events_response::Response::End(end_frame) => {
                assert!(!end, "duplicate end frame");
                end = true;
                end_cursor = end_frame.cursor.clone();
                assert!(
                    end_cursor.is_some(),
                    "list_events end frame should include cursor"
                );
                end_reason = Some(
                    QueryEndReason::try_from(end_frame.reason)
                        .expect("valid list_events end reason"),
                );
            }
            other => panic!("unexpected list_events response frame: {other:?}"),
        }
    }
    EventsResult {
        events,
        end,
        end_cursor,
        end_reason,
    }
}

async fn list_checkpoints_result(
    client: &mut KvLedgerServiceClient<Channel>,
    request: ListCheckpointsRequest,
) -> CheckpointsResult {
    let mut stream = client.list_checkpoints(request).await.unwrap().into_inner();
    let mut checkpoints = Vec::new();
    let mut end = false;
    let mut end_cursor = None;
    let mut end_reason = None;
    while let Some(response) = stream.message().await.unwrap() {
        match response.response.expect("list_checkpoints response frame") {
            list_checkpoints_response::Response::Item(item) => {
                assert!(!end, "item frame after end");
                checkpoints.push(item);
            }
            list_checkpoints_response::Response::End(end_frame) => {
                assert!(!end, "duplicate end frame");
                end = true;
                end_cursor = end_frame.cursor.clone();
                assert!(
                    end_cursor.is_some(),
                    "list_checkpoints end frame should include cursor"
                );
                end_reason = Some(
                    QueryEndReason::try_from(end_frame.reason)
                        .expect("valid list_checkpoints end reason"),
                );
            }
            other => panic!("unexpected list_checkpoints response frame: {other:?}"),
        }
    }
    CheckpointsResult {
        checkpoints,
        end,
        end_cursor,
        end_reason,
    }
}

async fn expect_invalid_list_transactions(
    client: &mut KvLedgerServiceClient<Channel>,
    request: ListTransactionsRequest,
) {
    let err = client
        .list_transactions(request)
        .await
        .expect_err("request should be InvalidArgument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

async fn expect_invalid_list_events(
    client: &mut KvLedgerServiceClient<Channel>,
    request: ListEventsRequest,
) {
    let err = client
        .list_events(request)
        .await
        .expect_err("request should be InvalidArgument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

async fn expect_invalid_list_checkpoints(
    client: &mut KvLedgerServiceClient<Channel>,
    request: ListCheckpointsRequest,
) {
    let err = client
        .list_checkpoints(request)
        .await
        .expect_err("request should be InvalidArgument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

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

fn authenticated_event_pkg_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/event/authenticated_event")
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

/// Execute a `transfer_sui(sender, None)` self-transfer and return `(tx_digest, updated_gas_ref)`.
async fn transfer_self(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &AccountKeyPair,
    gas: ObjectRef,
) -> (sui_types::digests::TransactionDigest, ObjectRef) {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender, None);
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (fx, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("transfer failed");
    assert!(err.is_none(), "transfer failed: {err:?}");
    let digest = *fx.transaction_digest();
    let new_gas = fx
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| *id == gas.0)
        .map(|((id, version, digest), _)| (id, version, digest))
        .expect("gas mutated");
    (digest, new_gas)
}

/// Run a transfer tx from `sender` and seal it into its own checkpoint.
/// Returns the new gas ObjectRef after the transfer.
async fn transfer_in_own_checkpoint(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &AccountKeyPair,
    gas: ObjectRef,
) -> ObjectRef {
    let (_, new_gas) = transfer_self(cluster, sender, kp, gas).await;
    cluster.create_checkpoint().await;
    new_gas
}

// --- Filter builders ---

fn tx_filter(literals: Vec<TransactionLiteral>) -> TransactionFilter {
    tx_filter_terms(vec![literals])
}

fn tx_filter_terms(terms: Vec<Vec<TransactionLiteral>>) -> TransactionFilter {
    let terms = terms
        .into_iter()
        .map(|literals| {
            let mut term = TransactionTerm::default();
            term.literals = literals;
            term
        })
        .collect();
    let mut filter = TransactionFilter::default();
    filter.terms = terms;
    filter
}

fn tx_or(terms: Vec<Vec<TransactionLiteral>>) -> TransactionFilter {
    tx_filter_terms(terms)
}

fn ev_filter(literals: Vec<EventLiteral>) -> EventFilter {
    ev_filter_terms(vec![literals])
}

fn ev_filter_terms(terms: Vec<Vec<EventLiteral>>) -> EventFilter {
    let terms = terms
        .into_iter()
        .map(|literals| {
            let mut term = EventTerm::default();
            term.literals = literals;
            term
        })
        .collect();
    let mut filter = EventFilter::default();
    filter.terms = terms;
    filter
}

fn ev_or(terms: Vec<Vec<EventLiteral>>) -> EventFilter {
    ev_filter_terms(terms)
}

fn tx_missing_include_filter(addr: SuiAddress) -> TransactionFilter {
    let mut term = TransactionTerm::default();
    term.literals = vec![tx_not_sender_literal(addr)];
    let mut filter = TransactionFilter::default();
    filter.terms = vec![term];
    filter
}

fn ev_missing_include_filter(addr: SuiAddress) -> EventFilter {
    let mut term = EventTerm::default();
    term.literals = vec![ev_not_sender_literal(addr)];
    let mut filter = EventFilter::default();
    filter.terms = vec![term];
    filter
}

fn tx_include(predicate: transaction_predicate::Predicate) -> TransactionLiteral {
    let mut p = TransactionPredicate::default();
    p.predicate = Some(predicate);
    let mut literal = TransactionLiteral::default();
    literal.polarity = Some(transaction_literal::Polarity::Include(p));
    literal
}

fn tx_exclude(predicate: transaction_predicate::Predicate) -> TransactionLiteral {
    let mut p = TransactionPredicate::default();
    p.predicate = Some(predicate);
    let mut literal = TransactionLiteral::default();
    literal.polarity = Some(transaction_literal::Polarity::Exclude(p));
    literal
}

fn tx_sender_literal(addr: SuiAddress) -> TransactionLiteral {
    let mut s = SenderFilter::default();
    s.address = Some(addr.to_string());
    tx_include(transaction_predicate::Predicate::Sender(s))
}

fn tx_not_sender_literal(addr: SuiAddress) -> TransactionLiteral {
    let mut s = SenderFilter::default();
    s.address = Some(addr.to_string());
    tx_exclude(transaction_predicate::Predicate::Sender(s))
}

fn tx_move_call_literal(path: &str) -> TransactionLiteral {
    let mut mc = MoveCallFilter::default();
    mc.function = Some(path.to_string());
    tx_include(transaction_predicate::Predicate::MoveCall(mc))
}

fn tx_emit_module_literal(path: &str) -> TransactionLiteral {
    let mut em = EmitModuleFilter::default();
    em.module = Some(path.to_string());
    tx_include(transaction_predicate::Predicate::EmitModule(em))
}

fn tx_event_type_literal(path: &str) -> TransactionLiteral {
    let mut et = EventTypeFilter::default();
    et.r#type = Some(path.to_string());
    tx_include(transaction_predicate::Predicate::EventType(et))
}

fn tx_event_stream_head_literal(stream_id: ObjectID) -> TransactionLiteral {
    let mut esh = EventStreamHeadFilter::default();
    esh.stream_id = Some(stream_id.to_canonical_string(true));
    tx_include(transaction_predicate::Predicate::EventStreamHead(esh))
}

fn tx_sender(addr: SuiAddress) -> TransactionFilter {
    tx_filter(vec![tx_sender_literal(addr)])
}

fn tx_move_call(path: &str) -> TransactionFilter {
    tx_filter(vec![tx_move_call_literal(path)])
}

fn tx_emit_module(path: &str) -> TransactionFilter {
    tx_filter(vec![tx_emit_module_literal(path)])
}

fn tx_event_type(path: &str) -> TransactionFilter {
    tx_filter(vec![tx_event_type_literal(path)])
}

fn tx_event_stream_head(stream_id: ObjectID) -> TransactionFilter {
    tx_filter(vec![tx_event_stream_head_literal(stream_id)])
}

fn tx_and(filters: Vec<TransactionFilter>) -> TransactionFilter {
    let mut literals = Vec::new();
    for filter in filters {
        for term in filter.terms {
            literals.extend(term.literals);
        }
    }
    tx_filter(literals)
}

fn ev_include(predicate: event_predicate::Predicate) -> EventLiteral {
    let mut p = EventPredicate::default();
    p.predicate = Some(predicate);
    let mut literal = EventLiteral::default();
    literal.polarity = Some(event_literal::Polarity::Include(p));
    literal
}

fn ev_exclude(predicate: event_predicate::Predicate) -> EventLiteral {
    let mut p = EventPredicate::default();
    p.predicate = Some(predicate);
    let mut literal = EventLiteral::default();
    literal.polarity = Some(event_literal::Polarity::Exclude(p));
    literal
}

fn ev_sender_literal(addr: SuiAddress) -> EventLiteral {
    let mut s = SenderFilter::default();
    s.address = Some(addr.to_string());
    ev_include(event_predicate::Predicate::Sender(s))
}

fn ev_not_sender_literal(addr: SuiAddress) -> EventLiteral {
    let mut s = SenderFilter::default();
    s.address = Some(addr.to_string());
    ev_exclude(event_predicate::Predicate::Sender(s))
}

fn ev_event_stream_head_literal(stream_id: ObjectID) -> EventLiteral {
    let mut esh = EventStreamHeadFilter::default();
    esh.stream_id = Some(stream_id.to_canonical_string(true));
    ev_include(event_predicate::Predicate::EventStreamHead(esh))
}

fn ev_sender(addr: SuiAddress) -> EventFilter {
    ev_filter(vec![ev_sender_literal(addr)])
}

fn ev_emit_module(path: &str) -> EventFilter {
    let mut em = EmitModuleFilter::default();
    em.module = Some(path.to_string());
    ev_filter(vec![ev_include(event_predicate::Predicate::EmitModule(em))])
}

fn ev_event_type(path: &str) -> EventFilter {
    let mut et = EventTypeFilter::default();
    et.r#type = Some(path.to_string());
    ev_filter(vec![ev_include(event_predicate::Predicate::EventType(et))])
}

fn ev_event_stream_head(stream_id: ObjectID) -> EventFilter {
    ev_filter(vec![ev_event_stream_head_literal(stream_id)])
}

fn ev_and(filters: Vec<EventFilter>) -> EventFilter {
    let mut literals = Vec::new();
    for filter in filters {
        for term in filter.terms {
            literals.extend(term.literals);
        }
    }
    ev_filter(literals)
}

// --- Tests ---

#[tokio::test]
async fn test_json_read_mask() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    let (pkg_id, gas) =
        publish_package(&mut cluster, sender, &kp, gas, emit_test_event_pkg_path()).await;
    let (event_tx_digest, _) = call_move(
        &mut cluster,
        sender,
        &kp,
        gas,
        pkg_id,
        "emit_test_event",
        "emit_test_event",
    )
    .await;

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

    let (tx_digest, _) = transfer_self(&mut cluster, sender, &kp, gas).await;

    cluster.create_checkpoint().await;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest", "checkpoint"]));
    req.options = Some(query_options(100));

    let resp = list_transactions_result(&mut client, req).await;
    assert_transaction_cursors(&resp);

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
    for result in &resp.transactions {
        assert!(
            result
                .transaction
                .as_ref()
                .and_then(|tx| tx.checkpoint)
                .is_some(),
            "transaction checkpoint should be present when requested"
        );
    }

    // Results should be ordered by checkpoint; transaction sequence ordering
    // within the checkpoint is covered by options/resume tests below.
    for w in resp.transactions.windows(2) {
        let a_checkpoint = w[0]
            .transaction
            .as_ref()
            .and_then(|tx| tx.checkpoint)
            .expect("transaction checkpoint should be present");
        let b_checkpoint = w[1]
            .transaction
            .as_ref()
            .and_then(|tx| tx.checkpoint)
            .expect("transaction checkpoint should be present");
        assert!(
            a_checkpoint <= b_checkpoint,
            "results should be ordered by checkpoint: {a_checkpoint} > {b_checkpoint}"
        );
    }
}

#[tokio::test]
async fn test_list_transactions_with_sender_filter() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    transfer_self(&mut cluster, sender, &kp, gas).await;

    cluster.create_checkpoint().await;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Filter by our sender.
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(100));

    let resp = list_transactions_result(&mut client, req).await;
    assert_transaction_cursors(&resp);
    assert!(
        !resp.transactions.is_empty(),
        "expected at least 1 transaction from sender"
    );
}

#[tokio::test]
async fn test_list_transactions_query_options() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, mut gas) = cluster.funded_account(20 * DEFAULT_GAS_BUDGET).unwrap();

    // Execute several transactions to ensure options.
    for _ in 0..3 {
        let (_, new_gas) = transfer_self(&mut cluster, sender, &kp, gas).await;
        gas = new_gas;
    }

    let tx_checkpoint = cluster.create_checkpoint().await;
    let tx_start = tx_checkpoint.sequence_number;
    let tx_end = tx_start + 1;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // First response: limit_items=2
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(tx_start);
    req.end_checkpoint = Some(tx_end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(2));

    let response1 = list_transactions_result(&mut client, req).await;
    assert_eq!(
        response1.transactions.len(),
        2,
        "first response should have 2 items"
    );
    assert_item_limit_end(response1.end, response1.end_reason);
    assert_transaction_cursors(&response1);
    let cursor = transaction_end_cursor(&response1, "first response should have an end cursor");

    // Second response using the last item cursor.
    let mut req2 = ListTransactionsRequest::default();
    req2.read_mask = Some(FieldMask::from_paths(["digest"]));
    req2.start_checkpoint = Some(tx_start);
    req2.end_checkpoint = Some(tx_end);
    req2.filter = Some(tx_sender(sender));
    req2.options = Some(query_options_after(2, cursor));

    let response2 = list_transactions_result(&mut client, req2).await;
    assert_eq!(
        response2.transactions.len(),
        1,
        "final response should have 1 item"
    );
    assert!(
        response2.end,
        "short final transaction response should include end frame"
    );
    assert_eq!(response2.end_reason, Some(QueryEndReason::CheckpointBound));
    assert_transaction_cursors(&response2);
    let final_cursor = last_transaction_cursor(&response2, "final response should have a cursor");

    let mut req3 = ListTransactionsRequest::default();
    req3.read_mask = Some(FieldMask::from_paths(["digest"]));
    req3.start_checkpoint = Some(tx_start);
    req3.end_checkpoint = Some(tx_end);
    req3.filter = Some(tx_sender(sender));
    req3.options = Some(query_options_after(2, final_cursor));
    let response3 = list_transactions_result(&mut client, req3).await;
    assert!(
        response3.transactions.is_empty(),
        "cursor after final transaction should return no items"
    );
    assert!(
        response3.end,
        "cursor after final transaction should include end frame"
    );
    assert_eq!(response3.end_reason, Some(QueryEndReason::CheckpointBound));

    // No overlap between responses.
    let response1_digests: Vec<_> = response1
        .transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
        .collect();
    let response2_digests: Vec<_> = response2
        .transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
        .collect();
    for d in &response2_digests {
        assert!(
            !response1_digests.contains(d),
            "response2 should not overlap with response1"
        );
    }

    // Reverse options reads descending of the same bounded result set.
    let mut reverse_req = ListTransactionsRequest::default();
    reverse_req.read_mask = Some(FieldMask::from_paths(["digest"]));
    reverse_req.start_checkpoint = Some(tx_start);
    reverse_req.end_checkpoint = Some(tx_end);
    reverse_req.filter = Some(tx_sender(sender));
    reverse_req.options = Some(query_options_descending(2));
    let reverse1 = list_transactions_result(&mut client, reverse_req).await;
    assert_eq!(reverse1.transactions.len(), 2, "reverse response size");
    assert_item_limit_end(reverse1.end, reverse1.end_reason);
    assert_transaction_cursors(&reverse1);
    let cursor = transaction_end_cursor(
        &reverse1,
        "first reverse response should have an end cursor",
    );
    let mut reverse_req2 = ListTransactionsRequest::default();
    reverse_req2.read_mask = Some(FieldMask::from_paths(["digest"]));
    reverse_req2.start_checkpoint = Some(tx_start);
    reverse_req2.end_checkpoint = Some(tx_end);
    reverse_req2.filter = Some(tx_sender(sender));
    reverse_req2.options = Some(query_options_descending_before(2, cursor));
    let reverse2 = list_transactions_result(&mut client, reverse_req2).await;
    assert_eq!(
        reverse2.transactions.len(),
        1,
        "final reverse response should have 1 item"
    );
    assert!(
        reverse2.end,
        "short final reverse transaction response should include end frame"
    );
    assert_transaction_cursors(&reverse2);
    let reverse1_digests: Vec<_> = reverse1
        .transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
        .collect();
    let reverse2_digests: Vec<_> = reverse2
        .transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
        .collect();
    for d in &reverse2_digests {
        assert!(
            !reverse1_digests.contains(d),
            "second reverse response should not overlap with response1"
        );
    }

    let mut exact_req = ListTransactionsRequest::default();
    exact_req.read_mask = Some(FieldMask::from_paths(["digest"]));
    exact_req.start_checkpoint = Some(tx_start);
    exact_req.end_checkpoint = Some(tx_end);
    exact_req.filter = Some(tx_sender(sender));
    exact_req.options = Some(query_options(3));
    let exact_result = list_transactions_result(&mut client, exact_req).await;
    assert_eq!(exact_result.transactions.len(), 3, "exact response size");
    assert_item_limit_end(exact_result.end, exact_result.end_reason);
    let exact_first_cursor = first_transaction_cursor(
        &exact_result,
        "exact transaction response should have a first cursor",
    );
    let exact_cursor = last_transaction_cursor(
        &exact_result,
        "exact transaction response should have a cursor",
    );

    let mut bounded_req = ListTransactionsRequest::default();
    bounded_req.read_mask = Some(FieldMask::from_paths(["digest"]));
    bounded_req.start_checkpoint = Some(tx_start);
    bounded_req.end_checkpoint = Some(tx_end);
    bounded_req.filter = Some(tx_sender(sender));
    bounded_req.options = Some(query_options_between(
        3,
        exact_first_cursor.clone(),
        exact_cursor.clone(),
    ));
    let bounded = list_transactions_result(&mut client, bounded_req).await;
    assert_eq!(
        bounded.transactions.len(),
        1,
        "exclusive transaction cursor bounds should leave the middle item"
    );
    assert_eq!(bounded.end_reason, Some(QueryEndReason::CursorBound));

    let mut bounded_desc_req = ListTransactionsRequest::default();
    bounded_desc_req.read_mask = Some(FieldMask::from_paths(["digest"]));
    bounded_desc_req.start_checkpoint = Some(tx_start);
    bounded_desc_req.end_checkpoint = Some(tx_end);
    bounded_desc_req.filter = Some(tx_sender(sender));
    bounded_desc_req.options = Some(query_options_between_descending(
        3,
        exact_first_cursor,
        exact_cursor.clone(),
    ));
    let bounded_desc = list_transactions_result(&mut client, bounded_desc_req).await;
    assert_eq!(
        bounded_desc.transactions.len(),
        1,
        "descending exclusive transaction cursor bounds should leave the middle item"
    );
    assert_eq!(bounded_desc.end_reason, Some(QueryEndReason::CursorBound));

    let mut exact_next_req = ListTransactionsRequest::default();
    exact_next_req.read_mask = Some(FieldMask::from_paths(["digest"]));
    exact_next_req.start_checkpoint = Some(tx_start);
    exact_next_req.end_checkpoint = Some(tx_end);
    exact_next_req.filter = Some(tx_sender(sender));
    exact_next_req.options = Some(query_options_after(3, exact_cursor));
    let exact_next_result = list_transactions_result(&mut client, exact_next_req).await;
    assert!(
        exact_next_result.transactions.is_empty(),
        "response after exact-size transaction result should be empty"
    );
    assert!(
        exact_next_result.end,
        "response after exact-size transaction result should include end frame"
    );
    assert_eq!(
        exact_next_result.end_reason,
        Some(QueryEndReason::CheckpointBound)
    );
}

#[tokio::test]
async fn test_list_events_unfiltered() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    let (pkg_id, gas) =
        publish_package(&mut cluster, sender, &kp, gas, emit_test_event_pkg_path()).await;
    let (event_tx_digest, _) = call_move(
        &mut cluster,
        sender,
        &kp,
        gas,
        pkg_id,
        "emit_test_event",
        "emit_test_event",
    )
    .await;

    cluster.create_checkpoint().await;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.options = Some(query_options(100));

    let resp = list_events_result(&mut client, req).await;
    assert_event_cursors(&resp);

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

    let (pkg_id, gas) =
        publish_package(&mut cluster, sender, &kp, gas, emit_test_event_pkg_path()).await;
    call_move(
        &mut cluster,
        sender,
        &kp,
        gas,
        pkg_id,
        "emit_test_event",
        "emit_test_event",
    )
    .await;

    cluster.create_checkpoint().await;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Filter by emit_module matching our package.
    let mut emit_mod = sui_rpc::proto::sui::rpc::v2alpha::EmitModuleFilter::default();
    emit_mod.module = Some(format!(
        "{}::emit_test_event",
        pkg_id.to_canonical_string(true)
    ));
    let filter = ev_filter(vec![ev_include(event_predicate::Predicate::EmitModule(
        emit_mod,
    ))]);

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(filter);
    req.options = Some(query_options(100));

    let resp = list_events_result(&mut client, req).await;
    assert_event_cursors(&resp);
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
async fn test_list_events_query_options() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    let (pkg_id, mut gas) =
        publish_package(&mut cluster, sender, &kp, gas, emit_test_event_pkg_path()).await;

    // Emit events from 3 separate transactions.
    for _ in 0..3 {
        let (_, new_gas) = call_move(
            &mut cluster,
            sender,
            &kp,
            gas,
            pkg_id,
            "emit_test_event",
            "emit_test_event",
        )
        .await;
        gas = new_gas;
    }

    let event_checkpoint = cluster.create_checkpoint().await;
    let event_start = event_checkpoint.sequence_number;
    let event_end = event_start + 1;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Use emit_module filter to find only events from our package.
    let mut emit_mod = sui_rpc::proto::sui::rpc::v2alpha::EmitModuleFilter::default();
    emit_mod.module = Some(pkg_id.to_canonical_string(true));
    let ev_filter = ev_filter(vec![ev_include(event_predicate::Predicate::EmitModule(
        emit_mod,
    ))]);

    // Paginate with limit_items=1.
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(event_start);
    req.end_checkpoint = Some(event_end);
    req.filter = Some(ev_filter.clone());
    req.options = Some(query_options(1));

    let response1 = list_events_result(&mut client, req).await;
    assert_eq!(
        response1.events.len(),
        1,
        "first response should have 1 event"
    );
    assert_item_limit_end(response1.end, response1.end_reason);
    assert_event_cursors(&response1);
    let cursor = event_end_cursor(&response1, "first response should have an end cursor");

    let mut req2 = ListEventsRequest::default();
    req2.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req2.start_checkpoint = Some(event_start);
    req2.end_checkpoint = Some(event_end);
    req2.filter = Some(ev_filter.clone());
    req2.options = Some(query_options_after(1, cursor));

    let response2 = list_events_result(&mut client, req2).await;
    assert_eq!(
        response2.events.len(),
        1,
        "second response should have 1 event"
    );
    assert_event_cursors(&response2);

    // Events on response2 should be different from response1.
    let response1_cursor = &response1.events[0].cursor;
    let response2_cursor = &response2.events[0].cursor;
    assert_ne!(
        response1_cursor, response2_cursor,
        "responses should have different cursors"
    );

    let mut reverse_req = ListEventsRequest::default();
    reverse_req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    reverse_req.start_checkpoint = Some(event_start);
    reverse_req.end_checkpoint = Some(event_end);
    reverse_req.filter = Some(ev_filter.clone());
    reverse_req.options = Some(query_options_descending(1));
    let reverse1 = list_events_result(&mut client, reverse_req).await;
    assert_eq!(reverse1.events.len(), 1, "first reverse response size");
    assert_item_limit_end(reverse1.end, reverse1.end_reason);
    assert_event_cursors(&reverse1);
    let cursor = event_end_cursor(
        &reverse1,
        "first reverse response should have an end cursor",
    );
    let mut reverse_req2 = ListEventsRequest::default();
    reverse_req2.read_mask = Some(FieldMask::from_paths(["event_type"]));
    reverse_req2.start_checkpoint = Some(event_start);
    reverse_req2.end_checkpoint = Some(event_end);
    reverse_req2.filter = Some(ev_filter.clone());
    reverse_req2.options = Some(query_options_descending_before(1, cursor));
    let reverse2 = list_events_result(&mut client, reverse_req2).await;
    assert_eq!(reverse2.events.len(), 1, "second reverse response size");
    assert_event_cursors(&reverse2);

    assert_ne!(
        reverse1.events[0].transaction_digest, reverse2.events[0].transaction_digest,
        "reverse responses should move backward"
    );

    let mut exact_req = ListEventsRequest::default();
    exact_req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    exact_req.start_checkpoint = Some(event_start);
    exact_req.end_checkpoint = Some(event_end);
    exact_req.filter = Some(ev_filter.clone());
    exact_req.options = Some(query_options(3));
    let exact_result = list_events_result(&mut client, exact_req).await;
    assert_eq!(exact_result.events.len(), 3, "exact event response size");
    assert_item_limit_end(exact_result.end, exact_result.end_reason);
    let exact_first_cursor = first_event_cursor(
        &exact_result,
        "exact event response should have a first cursor",
    );
    let exact_cursor =
        last_event_cursor(&exact_result, "exact event response should have a cursor");

    let mut bounded_req = ListEventsRequest::default();
    bounded_req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    bounded_req.start_checkpoint = Some(event_start);
    bounded_req.end_checkpoint = Some(event_end);
    bounded_req.filter = Some(ev_filter.clone());
    bounded_req.options = Some(query_options_between(
        3,
        exact_first_cursor.clone(),
        exact_cursor.clone(),
    ));
    let bounded = list_events_result(&mut client, bounded_req).await;
    assert_eq!(
        bounded.events.len(),
        1,
        "exclusive event cursor bounds should leave the middle event"
    );
    assert_eq!(bounded.end_reason, Some(QueryEndReason::CursorBound));

    let mut bounded_desc_req = ListEventsRequest::default();
    bounded_desc_req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    bounded_desc_req.start_checkpoint = Some(event_start);
    bounded_desc_req.end_checkpoint = Some(event_end);
    bounded_desc_req.filter = Some(ev_filter.clone());
    bounded_desc_req.options = Some(query_options_between_descending(
        3,
        exact_first_cursor,
        exact_cursor.clone(),
    ));
    let bounded_desc = list_events_result(&mut client, bounded_desc_req).await;
    assert_eq!(
        bounded_desc.events.len(),
        1,
        "descending exclusive event cursor bounds should leave the middle event"
    );
    assert_eq!(bounded_desc.end_reason, Some(QueryEndReason::CursorBound));

    let mut exact_next_req = ListEventsRequest::default();
    exact_next_req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    exact_next_req.start_checkpoint = Some(event_start);
    exact_next_req.end_checkpoint = Some(event_end);
    exact_next_req.filter = Some(ev_filter);
    exact_next_req.options = Some(query_options_after(3, exact_cursor));
    let exact_next_result = list_events_result(&mut client, exact_next_req).await;
    assert!(
        exact_next_result.events.is_empty(),
        "response after exact-size event result should be empty"
    );
    assert!(
        exact_next_result.end,
        "response after exact-size event result should include end frame"
    );
    assert_eq!(
        exact_next_result.end_reason,
        Some(QueryEndReason::CheckpointBound)
    );
}

#[tokio::test]
async fn test_list_transactions_or_prefix_and_event_predicates() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender_a, kp_a, gas_a) = cluster.funded_account(20 * DEFAULT_GAS_BUDGET).unwrap();
    let (sender_b, kp_b, gas_b) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();
    let (sender_c, kp_c, gas_c) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    let (pkg, gas_a) = publish_package(
        &mut cluster,
        sender_a,
        &kp_a,
        gas_a,
        generic_event_pkg_path(),
    )
    .await;
    let (digest_a, _) = call_move(
        &mut cluster,
        sender_a,
        &kp_a,
        gas_a,
        pkg,
        "generic_event",
        "emit_u64",
    )
    .await;
    let (digest_b, _) = call_move(
        &mut cluster,
        sender_b,
        &kp_b,
        gas_b,
        pkg,
        "generic_event",
        "emit_address",
    )
    .await;

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender_c, None);
    let data = TransactionData::new_programmable(
        sender_c,
        vec![gas_c],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );
    let (fx_c, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp_c]))
        .expect("C transfer failed");
    let digest_c = *fx_c.transaction_digest();

    cluster.create_checkpoint().await;

    let client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let fetch = |filter: TransactionFilter| {
        let mut c = client.clone();
        async move {
            let mut req = ListTransactionsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["digest"]));
            req.filter = Some(filter);
            req.options = Some(query_options(100));
            list_transactions_result(&mut c, req).await
        }
    };
    let digest_set = |result: TransactionsResult| {
        result
            .transactions
            .iter()
            .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
            .collect::<std::collections::HashSet<_>>()
    };

    let resp = fetch(tx_or(vec![
        vec![tx_sender_literal(sender_a)],
        vec![tx_sender_literal(sender_b)],
    ]))
    .await;
    let digests = digest_set(resp);
    assert!(
        digests.contains(&digest_a.to_string()) && digests.contains(&digest_b.to_string()),
        "sender OR should include A and B calls, got {digests:?}"
    );
    assert!(
        !digests.contains(&digest_c.to_string()),
        "sender OR should not include C transfer"
    );

    let pkg_path = pkg.to_canonical_string(true);
    let module_path = format!("{pkg_path}::generic_event");
    let emit_u64_path = format!("{module_path}::emit_u64");

    for filter in [tx_move_call(&pkg_path), tx_move_call(&module_path)] {
        let digests = digest_set(fetch(filter).await);
        assert!(
            digests.contains(&digest_a.to_string()) && digests.contains(&digest_b.to_string()),
            "package/module move-call prefixes should match both functions, got {digests:?}"
        );
        assert!(
            !digests.contains(&digest_c.to_string()),
            "move-call prefix should not include C transfer"
        );
    }

    let digests = digest_set(fetch(tx_move_call(&emit_u64_path)).await);
    assert!(
        digests.contains(&digest_a.to_string()) && !digests.contains(&digest_b.to_string()),
        "function-level move-call filter should match only emit_u64, got {digests:?}"
    );

    for filter in [tx_emit_module(&pkg_path), tx_emit_module(&module_path)] {
        let digests = digest_set(fetch(filter).await);
        assert!(
            digests.contains(&digest_a.to_string()) && digests.contains(&digest_b.to_string()),
            "tx emit_module package/module prefixes should match event-emitting txs, got {digests:?}"
        );
        assert!(
            !digests.contains(&digest_c.to_string()),
            "tx emit_module filter should not include C transfer"
        );
    }

    let u64_event_type = format!("{module_path}::GenericEvent<u64>");
    let digests = digest_set(fetch(tx_event_type(&u64_event_type)).await);
    assert!(
        digests.contains(&digest_a.to_string()) && !digests.contains(&digest_b.to_string()),
        "tx event_type filter should match only GenericEvent<u64>, got {digests:?}"
    );
}

#[tokio::test]
async fn test_list_events_sender_or_filter() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender_a, kp_a, gas_a) = cluster.funded_account(20 * DEFAULT_GAS_BUDGET).unwrap();
    let (sender_b, kp_b, gas_b) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();
    let (sender_c, kp_c, gas_c) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

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
    let (digest_c, _) = call_move(
        &mut cluster,
        sender_c,
        &kp_c,
        gas_c,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;

    cluster.create_checkpoint().await;

    let client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let fetch = |filter: EventFilter| {
        let mut c = client.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["event_type"]));
            req.filter = Some(filter);
            req.options = Some(query_options(100));
            list_events_result(&mut c, req).await
        }
    };
    let digest_set = |result: EventsResult| {
        result
            .events
            .iter()
            .filter_map(|e| e.transaction_digest.clone())
            .collect::<std::collections::HashSet<_>>()
    };

    let resp = fetch(ev_or(vec![
        vec![ev_sender_literal(sender_a)],
        vec![ev_sender_literal(sender_b)],
    ]))
    .await;
    let digests = digest_set(resp);
    assert!(
        digests.contains(&digest_a.to_string()) && digests.contains(&digest_b.to_string()),
        "event sender OR should include A and B events, got {digests:?}"
    );
    assert!(
        !digests.contains(&digest_c.to_string()),
        "event sender OR should not include C event"
    );
}

#[tokio::test]
async fn test_list_event_stream_head_filter() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    let (pkg, gas) = publish_package(
        &mut cluster,
        sender,
        &kp,
        gas,
        authenticated_event_pkg_path(),
    )
    .await;
    let (digest, _) = call_move(
        &mut cluster,
        sender,
        &kp,
        gas,
        pkg,
        "authenticated_event",
        "emit_both",
    )
    .await;

    cluster.create_checkpoint().await;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(ev_event_stream_head(pkg));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;

    assert_eq!(
        resp.events.len(),
        1,
        "event_stream_head should return only the authenticated event"
    );
    let event = &resp.events[0];
    let digest_string = digest.to_string();
    assert_eq!(
        event.transaction_digest.as_deref(),
        Some(digest_string.as_str())
    );
    let event_type = event
        .event
        .as_ref()
        .and_then(|event| event.event_type.as_deref())
        .expect("event_type should be present");
    assert!(
        event_type.contains("authenticated_event::AuthenticatedEvent"),
        "unexpected authenticated event type: {event_type}"
    );

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_event_stream_head(pkg));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    let digests: std::collections::HashSet<_> = resp
        .transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
        .collect();
    assert!(
        digests.contains(&digest.to_string()),
        "tx event_stream_head should include authenticated event tx, got {digests:?}"
    );
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
    let (digest_a_transfer, _) = transfer_self(&mut cluster, sender_a, &kp_a, gas_a).await;

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

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let move_call_path = format!(
        "{}::emit_test_event::emit_test_event",
        pkg.to_canonical_string(true)
    );
    let filter = tx_and(vec![tx_sender(sender_a), tx_move_call(&move_call_path)]);

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(filter);
    req.options = Some(query_options(100));

    let resp = list_transactions_result(&mut client, req).await;
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

    let (digest_a, _) = transfer_self(&mut cluster, sender_a, &kp_a, gas_a).await;
    let (digest_b, _) = transfer_self(&mut cluster, sender_b, &kp_b, gas_b).await;

    cluster.create_checkpoint().await;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Anchored DNF: sender A and not sender B.
    let filter = tx_filter(vec![
        tx_sender_literal(sender_a),
        tx_not_sender_literal(sender_b),
    ]);

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(filter);
    req.options = Some(query_options(100));

    let resp = list_transactions_result(&mut client, req).await;
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

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // AffectedAddress(B) should include the transfer.
    let mut r = AffectedAddressFilter::default();
    r.address = Some(sender_b.to_string());
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_filter(vec![tx_include(
        transaction_predicate::Predicate::AffectedAddress(r),
    )]));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
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
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_filter(vec![tx_include(
        transaction_predicate::Predicate::AffectedObject(ao),
    )]));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
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

    let client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let pkg_hex = pkg.to_canonical_string(true);

    let fetch = |filter: EventFilter| {
        let mut c = client.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["event_type"]));
            req.filter = Some(filter);
            req.options = Some(query_options(100));
            list_events_result(&mut c, req).await
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

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));
    let filter = ev_and(vec![ev_sender(sender_a), ev_emit_module(&module)]);

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(filter);
    req.options = Some(query_options(100));

    let resp = list_events_result(&mut client, req).await;
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
async fn test_list_events_combinator_or_not() {
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

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Anchored DNF: sender A and not sender B — exercises EventLiteral::Exclude.
    let filter = ev_filter(vec![
        ev_sender_literal(sender_a),
        ev_not_sender_literal(sender_b),
    ]);

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(filter);
    req.options = Some(query_options(100));

    let resp = list_events_result(&mut client, req).await;
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
        "B's event should be excluded by Not(Sender=B)"
    );
}

#[tokio::test]
async fn test_list_events_query_options_multi_event_tx() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    let (pkg, gas) =
        publish_package(&mut cluster, sender, &kp, gas, emit_test_event_pkg_path()).await;
    cluster.create_checkpoint().await;

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

    let emit_checkpoint = cluster.create_checkpoint().await;
    let emit_start = emit_checkpoint.sequence_number;
    let emit_end = emit_start + 1;

    let client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));

    let response = |filter: EventFilter, cursor: Option<prost::bytes::Bytes>| {
        let mut c = client.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["event_type"]));
            req.start_checkpoint = Some(emit_start);
            req.end_checkpoint = Some(emit_end);
            req.filter = Some(filter);
            req.options = Some(query_options_maybe_after(3, cursor));
            list_events_result(&mut c, req).await
        }
    };

    let f = ev_emit_module(&module);
    let r1 = response(f.clone(), None).await;
    assert_eq!(r1.events.len(), 3, "response 1 size");
    assert_item_limit_end(r1.end, r1.end_reason);
    assert_event_cursors(&r1);
    let r1_token = event_end_cursor(&r1, "response 1 should have an end cursor");

    let r2 = response(f.clone(), Some(r1_token)).await;
    assert_eq!(r2.events.len(), 3, "response 2 size");
    assert_item_limit_end(r2.end, r2.end_reason);
    assert_event_cursors(&r2);
    let r2_token = event_end_cursor(&r2, "response 2 should have an end cursor");

    let r3 = response(f, Some(r2_token)).await;
    assert_eq!(r3.events.len(), 2, "response 3 size");
    assert!(r3.end, "short response 3 should include end frame");
    assert_event_cursors(&r3);

    // No duplicates across responses, ordered by cursor.
    let mut all_cursors: Vec<_> = r1
        .events
        .iter()
        .chain(r2.events.iter())
        .chain(r3.events.iter())
        .map(|e| e.cursor.clone())
        .collect();
    let total = all_cursors.len();
    all_cursors.sort();
    all_cursors.dedup();
    assert_eq!(
        all_cursors.len(),
        total,
        "no duplicate cursors across responses"
    );
    assert_eq!(total, 8, "8 total events");

    let unfiltered_response = |cursor: Option<prost::bytes::Bytes>| {
        let mut c = client.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["event_type"]));
            req.start_checkpoint = Some(emit_start);
            req.end_checkpoint = Some(emit_end);
            req.options = Some(query_options_maybe_after(3, cursor));
            list_events_result(&mut c, req).await
        }
    };

    let up1 = unfiltered_response(None).await;
    assert_eq!(up1.events.len(), 3, "unfiltered response 1 size");
    assert_item_limit_end(up1.end, up1.end_reason);
    assert_event_cursors(&up1);
    let up1_token = event_end_cursor(&up1, "unfiltered response 1 should have an end cursor");

    let up2 = unfiltered_response(Some(up1_token)).await;
    assert_eq!(up2.events.len(), 3, "unfiltered response 2 size");
    assert_item_limit_end(up2.end, up2.end_reason);
    assert_event_cursors(&up2);
    let up2_token = event_end_cursor(&up2, "unfiltered response 2 should have an end cursor");

    let up3 = unfiltered_response(Some(up2_token)).await;
    assert_eq!(up3.events.len(), 2, "unfiltered response 3 size");
    assert!(
        up3.end,
        "short unfiltered event response should include end frame"
    );
    assert_event_cursors(&up3);

    let unfiltered_keys: Vec<_> = up1
        .events
        .iter()
        .chain(up2.events.iter())
        .chain(up3.events.iter())
        .map(|e| (e.transaction_digest.clone(), e.event_index))
        .collect();
    assert_eq!(unfiltered_keys.len(), 8, "8 total unfiltered events");
    let mut deduped = unfiltered_keys.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(
        deduped.len(),
        unfiltered_keys.len(),
        "no unfiltered duplicates"
    );

    let reverse_response = |filter: EventFilter, cursor: Option<prost::bytes::Bytes>| {
        let mut c = client.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["event_type"]));
            req.start_checkpoint = Some(emit_start);
            req.end_checkpoint = Some(emit_end);
            req.filter = Some(filter);
            req.options = Some(query_options_descending_maybe_before(3, cursor));
            list_events_result(&mut c, req).await
        }
    };

    let f = ev_emit_module(&module);
    let rp1 = reverse_response(f.clone(), None).await;
    assert_eq!(rp1.events.len(), 3, "reverse response 1 size");
    assert_item_limit_end(rp1.end, rp1.end_reason);
    assert_event_cursors(&rp1);
    let rp1_token = event_end_cursor(&rp1, "reverse response 1 should have an end cursor");

    let rp2 = reverse_response(f.clone(), Some(rp1_token)).await;
    assert_eq!(rp2.events.len(), 3, "reverse response 2 size");
    assert_item_limit_end(rp2.end, rp2.end_reason);
    assert_event_cursors(&rp2);
    let rp2_token = event_end_cursor(&rp2, "reverse response 2 should have an end cursor");

    let rp3 = reverse_response(f, Some(rp2_token)).await;
    assert_eq!(rp3.events.len(), 2, "reverse response 3 size");
    assert!(rp3.end, "short reverse response 3 should include end frame");
    assert_event_cursors(&rp3);

    let reverse_keys: Vec<_> = rp1
        .events
        .iter()
        .chain(rp2.events.iter())
        .chain(rp3.events.iter())
        .map(|e| (e.transaction_digest.clone(), e.event_index))
        .collect();
    assert_eq!(reverse_keys.len(), 8, "8 total reverse events");
    let mut deduped = reverse_keys.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(deduped.len(), reverse_keys.len(), "no reverse duplicates");

    let unfiltered_reverse_response = |cursor: Option<prost::bytes::Bytes>| {
        let mut c = client.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["event_type"]));
            req.start_checkpoint = Some(emit_start);
            req.end_checkpoint = Some(emit_end);
            req.options = Some(query_options_descending_maybe_before(3, cursor));
            list_events_result(&mut c, req).await
        }
    };

    let up1 = unfiltered_reverse_response(None).await;
    assert_eq!(up1.events.len(), 3, "unfiltered reverse response 1 size");
    assert_item_limit_end(up1.end, up1.end_reason);
    assert_event_cursors(&up1);
    let up1_token = event_end_cursor(
        &up1,
        "unfiltered reverse response 1 should have an end cursor",
    );

    let up2 = unfiltered_reverse_response(Some(up1_token)).await;
    assert_eq!(up2.events.len(), 3, "unfiltered reverse response 2 size");
    assert_item_limit_end(up2.end, up2.end_reason);
    assert_event_cursors(&up2);
    let up2_token = event_end_cursor(
        &up2,
        "unfiltered reverse response 2 should have an end cursor",
    );

    let up3 = unfiltered_reverse_response(Some(up2_token)).await;
    assert_eq!(up3.events.len(), 2, "unfiltered reverse response 3 size");
    assert!(
        up3.end,
        "short unfiltered reverse response 3 should include end frame"
    );
    assert_event_cursors(&up3);

    let unfiltered_reverse_keys: Vec<_> = up1
        .events
        .iter()
        .chain(up2.events.iter())
        .chain(up3.events.iter())
        .map(|e| (e.transaction_digest.clone(), e.event_index))
        .collect();
    assert_eq!(
        unfiltered_reverse_keys.len(),
        8,
        "8 total unfiltered reverse events"
    );
    let mut deduped = unfiltered_reverse_keys.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(
        deduped.len(),
        unfiltered_reverse_keys.len(),
        "no unfiltered reverse duplicates"
    );
}

#[tokio::test]
async fn test_list_filter_edge_cases() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // One trivial tx to have something indexed.
    transfer_self(&mut cluster, sender, &kp, gas).await;
    cluster.create_checkpoint().await;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Checkpoint range beyond what's indexed returns an empty stream.
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(9999);
    req.options = Some(query_options(10));
    let resp = list_transactions_result(&mut client, req).await;
    assert!(resp.transactions.is_empty(), "no txs beyond indexed range");
    assert!(resp.end, "empty tx range should include end frame");
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));
    assert_transaction_cursors(&resp);

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(9999);
    req.options = Some(query_options(10));
    let resp = list_events_result(&mut client, req).await;
    assert!(resp.events.is_empty(), "no events beyond indexed range");
    assert!(resp.end, "empty event range should include end frame");
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));
    assert_event_cursors(&resp);

    // Filter that matches nothing returns an empty stream.
    let never_sender: SuiAddress =
        "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
            .parse()
            .unwrap();
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_sender(never_sender));
    req.options = Some(query_options(10));
    let resp = list_transactions_result(&mut client, req).await;
    assert!(resp.transactions.is_empty(), "no-match filter");
    assert!(resp.end, "no-match tx filter should include end frame");
    assert_transaction_cursors(&resp);

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(ev_sender(never_sender));
    req.options = Some(query_options(10));
    let resp = list_events_result(&mut client, req).await;
    assert!(resp.events.is_empty(), "no-match event filter");
    assert!(resp.end, "no-match event filter should include end frame");
    assert_event_cursors(&resp);

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(tx_sender(never_sender));
    req.options = Some(query_options(10));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(resp.checkpoints.is_empty(), "no-match checkpoint filter");
    assert!(
        resp.end,
        "no-match checkpoint filter should include end frame"
    );
    assert_checkpoint_cursors(&resp);

    // Malformed MoveCall path (too many `::` parts) → InvalidArgument.
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(tx_move_call("0x1::a::b::c"));
    req.options = Some(query_options(10));
    let err = client
        .list_transactions(req)
        .await
        .expect_err("should be InvalidArgument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // Malformed EventType (generics without a name) → InvalidArgument.
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(ev_event_type("0x1<u64>"));
    req.options = Some(query_options(10));
    let err = client
        .list_events(req)
        .await
        .expect_err("should be InvalidArgument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.options = Some(query_options(10));
    let resp = list_transactions_result(&mut client, req).await;
    assert!(
        resp.end,
        "missing start_checkpoint should default to genesis"
    );

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.options = Some(query_options(10));
    let resp = list_transactions_result(&mut client, req).await;
    assert!(resp.end, "missing end_checkpoint should default to tip");

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(10);
    req.end_checkpoint = Some(9);
    req.options = Some(query_options(10));
    expect_invalid_list_transactions(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END + 1);
    req.options = Some(query_options(10));
    let resp = list_transactions_result(&mut client, req).await;
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    let mut bad_options = query_options(10);
    bad_options.after = Some(prost::bytes::Bytes::from_static(b"short"));
    req.options = Some(bad_options);
    expect_invalid_list_transactions(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    let mut bad_options = query_options(10);
    bad_options.ordering = 99;
    req.options = Some(bad_options);
    expect_invalid_list_transactions(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(TransactionFilter::default());
    req.options = Some(query_options(10));
    expect_invalid_list_transactions(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(tx_missing_include_filter(sender));
    req.options = Some(query_options(10));
    expect_invalid_list_transactions(&mut client, req).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(EventFilter::default());
    req.options = Some(query_options(10));
    expect_invalid_list_events(&mut client, req).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(ev_missing_include_filter(sender));
    req.options = Some(query_options(10));
    expect_invalid_list_events(&mut client, req).await;

    // ListCheckpoints: same validation contract as transactions/events.
    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.options = Some(query_options(10));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(
        resp.end,
        "missing start_checkpoint should default to genesis"
    );

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(10);
    req.end_checkpoint = Some(9);
    req.options = Some(query_options(10));
    expect_invalid_list_checkpoints(&mut client, req).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(TransactionFilter::default());
    req.options = Some(query_options(10));
    expect_invalid_list_checkpoints(&mut client, req).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(tx_missing_include_filter(sender));
    req.options = Some(query_options(10));
    expect_invalid_list_checkpoints(&mut client, req).await;
}

#[tokio::test]
async fn test_list_limit_items_over_cap_is_coerced() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();
    transfer_self(&mut cluster, sender, &kp, gas).await;
    cluster.create_checkpoint().await;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // Per proto, limit_items above each RPC's cap is coerced down, not rejected.
    let oversized = u32::MAX;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.options = Some(query_options(oversized));
    list_checkpoints_result(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.options = Some(query_options(oversized));
    list_transactions_result(&mut client, req).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.options = Some(query_options(oversized));
    list_events_result(&mut client, req).await;
}

// --- ListCheckpoints tests ---

#[tokio::test]
async fn test_list_checkpoints_unfiltered_range() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, mut gas) = cluster.funded_account(20 * DEFAULT_GAS_BUDGET).unwrap();

    // Three checkpoints, each containing one transfer tx.
    for _ in 0..3 {
        gas = transfer_in_own_checkpoint(&mut cluster, sender, &kp, gas).await;
    }

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.options = Some(query_options(100));

    let resp = list_checkpoints_result(&mut client, req).await;
    assert_checkpoint_cursors(&resp);

    // At least the genesis checkpoint plus the three we created.
    assert!(
        resp.checkpoints.len() >= 4,
        "expected at least 4 checkpoints, got {}",
        resp.checkpoints.len()
    );

    // Results should be ordered by sequence_number.
    for w in resp.checkpoints.windows(2) {
        let a = checkpoint_sequence(&w[0]);
        let b = checkpoint_sequence(&w[1]);
        assert!(
            a < b,
            "checkpoints should be strictly increasing: {a} >= {b}"
        );
    }
}

#[tokio::test]
async fn test_list_checkpoints_with_sender_filter() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender_a, kp_a, gas_a) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();
    let (sender_b, kp_b, gas_b) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // Checkpoint with sender_a's tx.
    transfer_in_own_checkpoint(&mut cluster, sender_a, &kp_a, gas_a).await;
    // Checkpoint with sender_b's tx.
    transfer_in_own_checkpoint(&mut cluster, sender_b, &kp_b, gas_b).await;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(tx_sender(sender_a));
    req.options = Some(query_options(100));

    let resp = list_checkpoints_result(&mut client, req).await;
    assert_checkpoint_cursors(&resp);
    assert!(
        !resp.checkpoints.is_empty(),
        "sender_a should match at least one checkpoint"
    );

    // Sanity check: the same query with sender_b also matches at least one cp,
    // and the matching cp seqs differ between the two queries.
    let a_seqs: std::collections::HashSet<u64> =
        resp.checkpoints.iter().map(checkpoint_sequence).collect();

    let mut req_b = ListCheckpointsRequest::default();
    req_b.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req_b.filter = Some(tx_sender(sender_b));
    req_b.options = Some(query_options(100));
    let resp_b = list_checkpoints_result(&mut client, req_b).await;
    assert_checkpoint_cursors(&resp_b);
    let b_seqs: std::collections::HashSet<u64> =
        resp_b.checkpoints.iter().map(checkpoint_sequence).collect();

    assert!(!b_seqs.is_empty(), "sender_b should match at least one cp");
    assert!(
        a_seqs.is_disjoint(&b_seqs),
        "sender_a and sender_b matched cps should be disjoint (each tx is in its own cp)"
    );
}

#[tokio::test]
async fn test_list_checkpoints_combinator_or() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender_a, kp_a, gas_a) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();
    let (sender_b, kp_b, gas_b) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();
    let (sender_c, kp_c, gas_c) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    transfer_in_own_checkpoint(&mut cluster, sender_a, &kp_a, gas_a).await;
    let cp_a = cluster.latest_checkpoint().await.unwrap().unwrap();
    transfer_in_own_checkpoint(&mut cluster, sender_b, &kp_b, gas_b).await;
    let cp_b = cluster.latest_checkpoint().await.unwrap().unwrap();
    transfer_in_own_checkpoint(&mut cluster, sender_c, &kp_c, gas_c).await;
    let cp_c = cluster.latest_checkpoint().await.unwrap().unwrap();

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(tx_or(vec![
        vec![tx_sender_literal(sender_a)],
        vec![tx_sender_literal(sender_b)],
    ]));
    req.options = Some(query_options(100));

    let resp = list_checkpoints_result(&mut client, req).await;
    assert_checkpoint_cursors(&resp);
    let seqs: std::collections::HashSet<u64> =
        resp.checkpoints.iter().map(checkpoint_sequence).collect();

    assert!(
        seqs.contains(&cp_a) && seqs.contains(&cp_b),
        "OR filter should include checkpoints for A and B, got {seqs:?}"
    );
    assert!(
        !seqs.contains(&cp_c),
        "OR filter should exclude checkpoint for C"
    );
}

#[tokio::test]
async fn test_list_checkpoints_query_options() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, mut gas) = cluster.funded_account(20 * DEFAULT_GAS_BUDGET).unwrap();

    // Three checkpoints, each with one transfer tx. The helper threads the
    // updated gas ref forward.
    let mut checkpoint_seqs = Vec::new();
    for _ in 0..3 {
        let (_, new_gas) = transfer_self(&mut cluster, sender, &kp, gas).await;
        let checkpoint = cluster.create_checkpoint().await;
        checkpoint_seqs.push(checkpoint.sequence_number);
        gas = new_gas;
    }
    let checkpoint_start = *checkpoint_seqs.first().unwrap();
    let checkpoint_end = checkpoint_seqs.last().unwrap() + 1;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // First response: limit_items=2.
    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(checkpoint_start);
    req.end_checkpoint = Some(checkpoint_end);
    req.options = Some(query_options(2));
    let response1 = list_checkpoints_result(&mut client, req).await;
    assert_eq!(
        response1.checkpoints.len(),
        2,
        "first response should have 2 items"
    );
    assert_item_limit_end(response1.end, response1.end_reason);
    assert_checkpoint_cursors(&response1);
    let cursor = checkpoint_end_cursor(
        &response1,
        "first checkpoint response should have an end cursor",
    );

    let response1_seqs: Vec<u64> = response1
        .checkpoints
        .iter()
        .map(checkpoint_sequence)
        .collect();

    // Second response using the last item cursor.
    let mut req2 = ListCheckpointsRequest::default();
    req2.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req2.start_checkpoint = Some(checkpoint_start);
    req2.end_checkpoint = Some(checkpoint_end);
    req2.options = Some(query_options_after(2, cursor));
    let response2 = list_checkpoints_result(&mut client, req2).await;
    assert_eq!(
        response2.checkpoints.len(),
        1,
        "final response should have 1 item"
    );
    assert!(
        response2.end,
        "short final checkpoint response should include end frame"
    );
    assert_eq!(response2.end_reason, Some(QueryEndReason::CheckpointBound));
    assert_checkpoint_cursors(&response2);

    let response2_seqs: Vec<u64> = response2
        .checkpoints
        .iter()
        .map(checkpoint_sequence)
        .collect();
    for s in &response2_seqs {
        assert!(
            !response1_seqs.contains(s),
            "response2 cp {s} should not appear in response1"
        );
        assert!(
            *s > *response1_seqs.last().unwrap(),
            "response2 should resume strictly after response1"
        );
    }

    let final_cursor =
        last_checkpoint_cursor(&response2, "final checkpoint response should have a cursor");
    let mut req3 = ListCheckpointsRequest::default();
    req3.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req3.start_checkpoint = Some(checkpoint_start);
    req3.end_checkpoint = Some(checkpoint_end);
    req3.options = Some(query_options_after(2, final_cursor));
    let response3 = list_checkpoints_result(&mut client, req3).await;
    assert!(
        response3.checkpoints.is_empty(),
        "cursor after final checkpoint should return no items"
    );
    assert!(
        response3.end,
        "cursor after final checkpoint should include end frame"
    );
    assert_eq!(response3.end_reason, Some(QueryEndReason::CursorBound));

    let mut reverse_req = ListCheckpointsRequest::default();
    reverse_req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    reverse_req.start_checkpoint = Some(checkpoint_start);
    reverse_req.end_checkpoint = Some(checkpoint_end);
    reverse_req.options = Some(query_options_descending(2));
    let reverse1 = list_checkpoints_result(&mut client, reverse_req).await;
    assert_eq!(reverse1.checkpoints.len(), 2, "reverse response size");
    assert_item_limit_end(reverse1.end, reverse1.end_reason);
    assert_checkpoint_cursors(&reverse1);
    let reverse1_seqs: Vec<_> = reverse1
        .checkpoints
        .iter()
        .map(checkpoint_sequence)
        .collect();
    assert!(
        reverse1_seqs.windows(2).all(|pair| pair[0] > pair[1]),
        "reverse checkpoints should be descending"
    );

    let cursor = checkpoint_end_cursor(
        &reverse1,
        "first reverse checkpoint response should have an end cursor",
    );
    let mut reverse_req2 = ListCheckpointsRequest::default();
    reverse_req2.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    reverse_req2.start_checkpoint = Some(checkpoint_start);
    reverse_req2.end_checkpoint = Some(checkpoint_end);
    reverse_req2.options = Some(query_options_descending_before(2, cursor));
    let reverse2 = list_checkpoints_result(&mut client, reverse_req2).await;
    assert_eq!(
        reverse2.checkpoints.len(),
        1,
        "final reverse checkpoint response should have 1 item"
    );
    assert!(
        reverse2.end,
        "short final reverse checkpoint response should include end frame"
    );
    assert_checkpoint_cursors(&reverse2);
    let reverse2_seqs: Vec<_> = reverse2
        .checkpoints
        .iter()
        .map(checkpoint_sequence)
        .collect();
    assert!(
        reverse2_seqs
            .iter()
            .all(|seq| *seq < *reverse1_seqs.last().unwrap()),
        "second reverse checkpoint response should resume before response1"
    );

    let mut exact_req = ListCheckpointsRequest::default();
    exact_req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    exact_req.start_checkpoint = Some(checkpoint_start);
    exact_req.end_checkpoint = Some(checkpoint_end);
    exact_req.options = Some(query_options(3));
    let exact_result = list_checkpoints_result(&mut client, exact_req).await;
    assert_eq!(
        exact_result.checkpoints.len(),
        3,
        "exact checkpoint response size"
    );
    assert_item_limit_end(exact_result.end, exact_result.end_reason);
    let exact_first_cursor = first_checkpoint_cursor(
        &exact_result,
        "exact checkpoint response should have a first cursor",
    );
    let exact_cursor = last_checkpoint_cursor(
        &exact_result,
        "exact checkpoint response should have a cursor",
    );

    let mut bounded_req = ListCheckpointsRequest::default();
    bounded_req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    bounded_req.start_checkpoint = Some(checkpoint_start);
    bounded_req.end_checkpoint = Some(checkpoint_end);
    bounded_req.options = Some(query_options_between(
        3,
        exact_first_cursor.clone(),
        exact_cursor.clone(),
    ));
    let bounded = list_checkpoints_result(&mut client, bounded_req).await;
    assert_eq!(
        bounded.checkpoints.len(),
        1,
        "exclusive checkpoint cursor bounds should leave the middle checkpoint"
    );
    assert_eq!(bounded.end_reason, Some(QueryEndReason::CursorBound));

    let mut bounded_desc_req = ListCheckpointsRequest::default();
    bounded_desc_req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    bounded_desc_req.start_checkpoint = Some(checkpoint_start);
    bounded_desc_req.end_checkpoint = Some(checkpoint_end);
    bounded_desc_req.options = Some(query_options_between_descending(
        3,
        exact_first_cursor,
        exact_cursor.clone(),
    ));
    let bounded_desc = list_checkpoints_result(&mut client, bounded_desc_req).await;
    assert_eq!(
        bounded_desc.checkpoints.len(),
        1,
        "descending exclusive checkpoint cursor bounds should leave the middle checkpoint"
    );
    assert_eq!(bounded_desc.end_reason, Some(QueryEndReason::CursorBound));

    let mut exact_next_req = ListCheckpointsRequest::default();
    exact_next_req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    exact_next_req.start_checkpoint = Some(checkpoint_start);
    exact_next_req.end_checkpoint = Some(checkpoint_end);
    exact_next_req.options = Some(query_options_after(3, exact_cursor));
    let exact_next_result = list_checkpoints_result(&mut client, exact_next_req).await;
    assert!(
        exact_next_result.checkpoints.is_empty(),
        "response after exact-size checkpoint result should be empty"
    );
    assert!(
        exact_next_result.end,
        "response after exact-size checkpoint result should include end frame"
    );
    assert_eq!(
        exact_next_result.end_reason,
        Some(QueryEndReason::CursorBound)
    );
}

#[tokio::test]
async fn test_list_checkpoints_combinator_and() {
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

    // Checkpoint 1: sender A + matching move call (the only intended match).
    let (_, gas_a) = call_move(
        &mut cluster,
        sender_a,
        &kp_a,
        gas_a,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;
    cluster.create_checkpoint().await;
    let cp_a_call = cluster
        .latest_checkpoint()
        .await
        .unwrap()
        .expect("checkpoint seq should exist after create_checkpoint");

    // Checkpoint 2: sender A + transfer (no move call).
    let _ = transfer_in_own_checkpoint(&mut cluster, sender_a, &kp_a, gas_a).await;
    let cp_a_transfer = cluster.latest_checkpoint().await.unwrap().unwrap();

    // Checkpoint 3: sender B + matching move call.
    let _ = call_move(
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
    let cp_b_call = cluster.latest_checkpoint().await.unwrap().unwrap();

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let move_call_path = format!(
        "{}::emit_test_event::emit_test_event",
        pkg.to_canonical_string(true)
    );
    let filter = tx_and(vec![tx_sender(sender_a), tx_move_call(&move_call_path)]);

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(filter);
    req.options = Some(query_options(100));

    let resp = list_checkpoints_result(&mut client, req).await;
    assert_checkpoint_cursors(&resp);
    let seqs: std::collections::HashSet<u64> =
        resp.checkpoints.iter().map(checkpoint_sequence).collect();

    assert!(
        seqs.contains(&cp_a_call),
        "expected cp containing A+call to match"
    );
    assert!(
        !seqs.contains(&cp_a_transfer),
        "cp containing only A+transfer must not match (no move call)"
    );
    assert!(
        !seqs.contains(&cp_b_call),
        "cp containing B+call must not match (wrong sender)"
    );
}

#[tokio::test]
async fn test_list_checkpoints_empty_range_past_watermark() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();
    transfer_in_own_checkpoint(&mut cluster, sender, &kp, gas).await;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    // start_checkpoint deliberately past the watermark; expect empty result.
    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(1_000_000);
    req.options = Some(query_options(10));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(resp.checkpoints.is_empty());
    assert!(resp.end, "empty checkpoint range should include end frame");
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));
    assert_checkpoint_cursors(&resp);
}

#[tokio::test]
async fn test_list_checkpoints_with_transactions_read_mask() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, mut gas) = cluster.funded_account(20 * DEFAULT_GAS_BUDGET).unwrap();

    // Two checkpoints, each containing one transfer tx. Capture the digests
    // so we can confirm the populated `transactions[].digest` matches.
    let mut expected_digests: Vec<sui_types::digests::TransactionDigest> = Vec::new();
    for _ in 0..2 {
        let (digest, new_gas) = transfer_self(&mut cluster, sender, &kp, gas).await;
        cluster.create_checkpoint().await;
        expected_digests.push(digest);
        gas = new_gas;
    }

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths([
        "sequence_number",
        "transactions.digest",
    ]));
    req.options = Some(query_options(100));

    let resp = list_checkpoints_result(&mut client, req).await;

    let returned_digests: std::collections::HashSet<String> = resp
        .checkpoints
        .iter()
        .flat_map(|cp| {
            let proto_cp = cp.checkpoint.as_ref().expect("checkpoint populated");
            proto_cp
                .transactions
                .iter()
                .filter_map(|tx| tx.digest.clone())
        })
        .collect();

    for expected in &expected_digests {
        assert!(
            returned_digests.contains(&expected.to_string()),
            "expected digest {expected} to appear in transactions[].digest, got {returned_digests:?}"
        );
    }
}

#[tokio::test]
async fn test_list_checkpoints_with_objects_read_mask() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, gas) = cluster.funded_account(10 * DEFAULT_GAS_BUDGET).unwrap();

    // One checkpoint with a transfer; capture the gas object id so we can
    // verify it shows up in objects[].object_id.
    let gas_id_string = gas.0.to_canonical_string(true);
    transfer_self(&mut cluster, sender, &kp, gas).await;
    cluster.create_checkpoint().await;

    let mut client = KvLedgerServiceClient::connect(cluster.kv_rpc_url().to_string())
        .await
        .unwrap();

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths([
        "sequence_number",
        "transactions.digest",
        "objects.objects.object_id",
        "objects.objects.version",
    ]));
    req.options = Some(query_options(100));

    let resp = list_checkpoints_result(&mut client, req).await;

    // At least one returned checkpoint should populate objects[] with the
    // gas object that the transfer mutated.
    let saw_gas_object = resp.checkpoints.iter().any(|cp| {
        let proto_cp = cp.checkpoint.as_ref().expect("checkpoint populated");
        proto_cp
            .objects
            .as_ref()
            .map(|os| {
                os.objects.iter().any(|o| {
                    o.object_id
                        .as_ref()
                        .is_some_and(|oid| oid == &gas_id_string)
                })
            })
            .unwrap_or(false)
    });
    assert!(
        saw_gas_object,
        "expected gas object {gas_id_string} to appear in some checkpoint's objects[]"
    );

    // The same checkpoint should also populate transactions[].digest, since
    // the heavy path fetches transactions even when only objects is in the
    // mask.
    let any_transactions = resp.checkpoints.iter().any(|cp| {
        let proto_cp = cp.checkpoint.as_ref().expect("checkpoint populated");
        proto_cp.transactions.iter().any(|tx| tx.digest.is_some())
    });
    assert!(
        any_transactions,
        "expected at least one tx.digest populated"
    );
}
