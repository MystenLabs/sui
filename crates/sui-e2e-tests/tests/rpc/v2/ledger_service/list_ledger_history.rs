// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::path::PathBuf;

use move_core_types::ident_str;
use prost::bytes::Bytes;
use prost_types::FieldMask;
use sui_macros::sim_test;
use sui_rpc::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient as V2LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2alpha::AffectedAddressFilter;
use sui_rpc::proto::sui::rpc::v2alpha::AffectedObjectFilter;
use sui_rpc::proto::sui::rpc::v2alpha::EmitModuleFilter;
use sui_rpc::proto::sui::rpc::v2alpha::EventFilter;
use sui_rpc::proto::sui::rpc::v2alpha::EventLiteral;
use sui_rpc::proto::sui::rpc::v2alpha::EventStreamHeadFilter;
use sui_rpc::proto::sui::rpc::v2alpha::EventTerm;
use sui_rpc::proto::sui::rpc::v2alpha::EventTypeFilter;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListEventsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2alpha::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2alpha::MoveCallFilter;
use sui_rpc::proto::sui::rpc::v2alpha::Ordering;
use sui_rpc::proto::sui::rpc::v2alpha::PackageWriteFilter;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::QueryOptions;
use sui_rpc::proto::sui::rpc::v2alpha::SenderFilter;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionFilter;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionLiteral;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionTerm;
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::event_literal;
use sui_rpc::proto::sui::rpc::v2alpha::ledger_service_client::LedgerServiceClient as AlphaLedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2alpha::transaction_literal;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tonic::transport::Channel;

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;
const DEFAULT_CHECKPOINT_RANGE_END: u64 = 3_000_000;

struct TransactionsResult {
    transactions: Vec<ListTransactionsResponse>,
    end: bool,
    end_cursor: Option<Bytes>,
    end_reason: Option<QueryEndReason>,
}

struct EventsResult {
    events: Vec<ListEventsResponse>,
    end: bool,
    end_cursor: Option<Bytes>,
    end_reason: Option<QueryEndReason>,
}

struct CheckpointsResult {
    checkpoints: Vec<ListCheckpointsResponse>,
    // Watermarks from payload-less frames (scan/terminal), in stream order.
    watermarks: Vec<Watermark>,
    end: bool,
    end_cursor: Option<Bytes>,
    end_reason: Option<QueryEndReason>,
}

fn query_options(limit_items: u32) -> QueryOptions {
    let mut options = QueryOptions::default();
    options.limit = Some(limit_items);
    options
}

fn query_options_after(limit_items: u32, after: Bytes) -> QueryOptions {
    let mut options = query_options(limit_items);
    options.after = Some(after);
    options
}

fn query_options_maybe_after(limit_items: u32, after: Option<Bytes>) -> QueryOptions {
    let mut options = query_options(limit_items);
    options.after = after;
    options
}

fn query_options_descending(limit_items: u32) -> QueryOptions {
    let mut options = query_options(limit_items);
    options.ordering = Some(Ordering::Descending as i32);
    options
}

fn query_options_descending_before(limit_items: u32, before: Bytes) -> QueryOptions {
    let mut options = query_options_descending(limit_items);
    options.before = Some(before);
    options
}

fn query_options_descending_maybe_before(limit_items: u32, before: Option<Bytes>) -> QueryOptions {
    let mut options = query_options_descending(limit_items);
    options.before = before;
    options
}

fn query_options_between(limit_items: u32, after: Bytes, before: Bytes) -> QueryOptions {
    let mut options = query_options(limit_items);
    options.after = Some(after);
    options.before = Some(before);
    options
}

fn query_options_between_descending(limit_items: u32, after: Bytes, before: Bytes) -> QueryOptions {
    let mut options = query_options_between(limit_items, after, before);
    options.ordering = Some(Ordering::Descending as i32);
    options
}

fn first_transaction_cursor(result: &TransactionsResult, message: &str) -> Bytes {
    result
        .transactions
        .first()
        .and_then(|item| item.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(message)
}

fn last_transaction_cursor(result: &TransactionsResult, message: &str) -> Bytes {
    result
        .transactions
        .last()
        .and_then(|item| item.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(message)
}

fn transaction_end_cursor(result: &TransactionsResult, message: &str) -> Bytes {
    result.end_cursor.clone().expect(message)
}

fn first_event_cursor(result: &EventsResult, message: &str) -> Bytes {
    result
        .events
        .first()
        .and_then(|item| item.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(message)
}

fn last_event_cursor(result: &EventsResult, message: &str) -> Bytes {
    result
        .events
        .last()
        .and_then(|item| item.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(message)
}

fn event_end_cursor(result: &EventsResult, message: &str) -> Bytes {
    result.end_cursor.clone().expect(message)
}

fn first_checkpoint_cursor(result: &CheckpointsResult, message: &str) -> Bytes {
    result
        .checkpoints
        .first()
        .and_then(|item| item.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(message)
}

fn last_checkpoint_cursor(result: &CheckpointsResult, message: &str) -> Bytes {
    result
        .checkpoints
        .last()
        .and_then(|item| item.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(message)
}

fn checkpoint_end_cursor(result: &CheckpointsResult, message: &str) -> Bytes {
    result.end_cursor.clone().expect(message)
}

fn assert_item_limit_end(end: bool, reason: Option<QueryEndReason>) {
    assert!(end, "item-limit response should include end frame");
    assert_eq!(reason, Some(QueryEndReason::ItemLimit));
}

fn item_has_cursor(watermark: Option<&Watermark>) -> bool {
    watermark.is_some_and(|w| w.cursor.is_some())
}

fn assert_transaction_cursors(result: &TransactionsResult) {
    for item in &result.transactions {
        assert!(
            item_has_cursor(item.watermark.as_ref()),
            "transaction item should have a watermark cursor"
        );
    }
}

fn assert_event_cursors(result: &EventsResult) {
    for item in &result.events {
        assert!(
            item_has_cursor(item.watermark.as_ref()),
            "event item should have a watermark cursor"
        );
    }
}

fn assert_checkpoint_cursors(result: &CheckpointsResult) {
    for item in &result.checkpoints {
        assert!(
            item_has_cursor(item.watermark.as_ref()),
            "checkpoint item should have a watermark cursor"
        );
    }
}

fn checkpoint_sequence(response: &ListCheckpointsResponse) -> u64 {
    response
        .checkpoint
        .as_ref()
        .and_then(|checkpoint| checkpoint.sequence_number)
        .expect("checkpoint sequence number should be populated")
}

fn transaction_digest_set(result: &TransactionsResult) -> HashSet<String> {
    result
        .transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
        .collect()
}

/// The event's ledger position now lives on the embedded `Event`, not the
/// response frame; these accessors read it back for assertions.
fn event_transaction_digest(item: &ListEventsResponse) -> Option<String> {
    item.event
        .as_ref()
        .and_then(|event| event.transaction_digest.clone())
}

fn event_checkpoint(item: &ListEventsResponse) -> Option<u64> {
    item.event.as_ref().and_then(|event| event.checkpoint)
}

fn event_index_of(item: &ListEventsResponse) -> Option<u32> {
    item.event.as_ref().and_then(|event| event.event_index)
}

/// Event read mask requesting the event type plus the ledger-position fields
/// (`checkpoint`, `transaction_digest`, `event_index`) that the list endpoint
/// only populates when they are asked for. Used by the event tests, which
/// assert on those positions.
fn event_type_and_position_mask() -> FieldMask {
    FieldMask::from_paths([
        "event_type",
        "checkpoint",
        "transaction_digest",
        "event_index",
    ])
}

fn event_digest_set(result: &EventsResult) -> HashSet<String> {
    result
        .events
        .iter()
        .filter_map(event_transaction_digest)
        .collect()
}

async fn list_transactions_result(
    client: &mut AlphaLedgerServiceClient<Channel>,
    request: ListTransactionsRequest,
) -> TransactionsResult {
    let mut stream = client
        .list_transactions(request)
        .await
        .unwrap()
        .into_inner();
    let mut transactions = Vec::new();
    let mut end = false;
    // Resume cursor is the latest watermark cursor seen, on an item or a
    // scan watermark frame — QueryEnd no longer carries one.
    let mut end_cursor = None;
    let mut end_reason = None;
    while let Some(response) = stream.message().await.unwrap() {
        assert!(!end, "frame after end");
        if let Some(cursor) = response.watermark.as_ref().and_then(|w| w.cursor.clone()) {
            end_cursor = Some(cursor);
        }
        if let Some(end_frame) = &response.end {
            end = true;
            end_reason = Some(end_frame.reason());
        }
        if response.transaction.is_some() {
            transactions.push(response);
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
    client: &mut AlphaLedgerServiceClient<Channel>,
    request: ListEventsRequest,
) -> EventsResult {
    let mut stream = client.list_events(request).await.unwrap().into_inner();
    let mut events = Vec::new();
    let mut end = false;
    // Resume cursor is the latest watermark cursor seen, on an item or a
    // scan watermark frame — QueryEnd no longer carries one.
    let mut end_cursor = None;
    let mut end_reason = None;
    while let Some(response) = stream.message().await.unwrap() {
        assert!(!end, "frame after end");
        if let Some(cursor) = response.watermark.as_ref().and_then(|w| w.cursor.clone()) {
            end_cursor = Some(cursor);
        }
        if let Some(end_frame) = &response.end {
            end = true;
            end_reason = Some(end_frame.reason());
        }
        if response.event.is_some() {
            events.push(response);
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
    client: &mut AlphaLedgerServiceClient<Channel>,
    request: ListCheckpointsRequest,
) -> CheckpointsResult {
    let mut stream = client.list_checkpoints(request).await.unwrap().into_inner();
    let mut checkpoints = Vec::new();
    let mut watermarks = Vec::new();
    let mut end = false;
    // Resume cursor is the latest watermark cursor seen, on an item or a
    // scan watermark frame — QueryEnd no longer carries one.
    let mut end_cursor = None;
    let mut end_reason = None;
    while let Some(response) = stream.message().await.unwrap() {
        assert!(!end, "frame after end");
        if let Some(cursor) = response.watermark.as_ref().and_then(|w| w.cursor.clone()) {
            end_cursor = Some(cursor);
        }
        if let Some(end_frame) = &response.end {
            end = true;
            end_reason = Some(end_frame.reason());
        }
        if response.checkpoint.is_some() {
            checkpoints.push(response);
        } else if let Some(watermark) = response.watermark {
            watermarks.push(watermark);
        }
    }
    CheckpointsResult {
        checkpoints,
        watermarks,
        end,
        end_cursor,
        end_reason,
    }
}

async fn expect_invalid_list_transactions(
    client: &mut AlphaLedgerServiceClient<Channel>,
    request: ListTransactionsRequest,
) {
    let err = client
        .list_transactions(request)
        .await
        .expect_err("request should be InvalidArgument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

async fn expect_invalid_list_events(
    client: &mut AlphaLedgerServiceClient<Channel>,
    request: ListEventsRequest,
) {
    let err = client
        .list_events(request)
        .await
        .expect_err("request should be InvalidArgument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

async fn expect_invalid_list_checkpoints(
    client: &mut AlphaLedgerServiceClient<Channel>,
    request: ListCheckpointsRequest,
) {
    let err = client
        .list_checkpoints(request)
        .await
        .expect_err("request should be InvalidArgument");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

async fn new_cluster() -> TestCluster {
    TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .with_rpc_config(sui_config::RpcConfig {
            enable_indexing: Some(true),
            ..Default::default()
        })
        .build()
        .await
}

async fn new_ledger_client(cluster: &TestCluster) -> AlphaLedgerServiceClient<Channel> {
    AlphaLedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap()
}

async fn latest_checkpoint_sequence(cluster: &TestCluster) -> u64 {
    let mut client = V2LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();
    client
        .get_checkpoint(
            GetCheckpointRequest::latest()
                .with_read_mask(FieldMask::from_paths(["sequence_number"])),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .and_then(|checkpoint| checkpoint.sequence_number)
        .expect("latest checkpoint sequence should be populated")
}

async fn publish_package(
    cluster: &TestCluster,
    sender: SuiAddress,
    path: PathBuf,
) -> (ObjectID, ExecutedTransaction) {
    super::super::publish_package(cluster, sender, path).await
}

fn emit_test_event_pkg_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rpc/data/ledger_history/event/emit_test_event")
}

fn authenticated_event_pkg_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rpc/data/ledger_history/event/authenticated_event")
}

fn generic_event_pkg_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rpc/data/ledger_history/event/generic_event")
}

async fn gas_object(cluster: &TestCluster, sender: SuiAddress) -> ObjectRef {
    cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .expect("sender should have a gas object")
}

async fn execute_programmable(
    cluster: &TestCluster,
    sender: SuiAddress,
    gas: ObjectRef,
    builder: ProgrammableTransactionBuilder,
) -> ExecutedTransaction {
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
    super::super::execute_transaction(&mut client, &transaction).await
}

async fn call_move(
    cluster: &TestCluster,
    sender: SuiAddress,
    pkg: ObjectID,
    module: &str,
    function: &str,
) -> ExecutedTransaction {
    let gas = gas_object(cluster, sender).await;
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.programmable_move_call(
        pkg,
        move_core_types::identifier::Identifier::new(module).unwrap(),
        move_core_types::identifier::Identifier::new(function).unwrap(),
        vec![],
        vec![],
    );
    execute_programmable(cluster, sender, gas, builder).await
}

async fn call_emit_many(
    cluster: &TestCluster,
    sender: SuiAddress,
    pkg: ObjectID,
    count: u64,
) -> ExecutedTransaction {
    let gas = gas_object(cluster, sender).await;
    let mut builder = ProgrammableTransactionBuilder::new();
    let arg = builder.pure(count).unwrap();
    builder.programmable_move_call(
        pkg,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_many").to_owned(),
        vec![],
        vec![arg],
    );
    execute_programmable(cluster, sender, gas, builder).await
}

async fn transfer_self(cluster: &TestCluster, sender: SuiAddress) -> ExecutedTransaction {
    let gas = gas_object(cluster, sender).await;
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender, None);
    execute_programmable(cluster, sender, gas, builder).await
}

async fn split_transfer(
    cluster: &TestCluster,
    sender: SuiAddress,
    recipient: SuiAddress,
) -> (ExecutedTransaction, ObjectID) {
    let gas = gas_object(cluster, sender).await;
    let affected_gas_id = gas.0;
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(recipient, Some(1_000_000));
    (
        execute_programmable(cluster, sender, gas, builder).await,
        affected_gas_id,
    )
}

fn tx_checkpoint(tx: &ExecutedTransaction) -> u64 {
    tx.checkpoint
        .expect("executed transaction should have checkpoint")
}

fn checkpoint_range(txs: &[&ExecutedTransaction]) -> (u64, u64) {
    let start = txs
        .iter()
        .map(|tx| tx_checkpoint(tx))
        .min()
        .expect("non-empty tx list");
    let end = txs
        .iter()
        .map(|tx| tx_checkpoint(tx))
        .max()
        .expect("non-empty tx list")
        + 1;
    (start, end)
}

fn tx_digest(tx: &ExecutedTransaction) -> String {
    tx.digest().to_owned()
}

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

fn tx_not_sender_only_filter(addr: SuiAddress) -> TransactionFilter {
    let mut term = TransactionTerm::default();
    term.literals = vec![tx_not_sender_literal(addr)];
    let mut filter = TransactionFilter::default();
    filter.terms = vec![term];
    filter
}

fn ev_not_sender_only_filter(addr: SuiAddress) -> EventFilter {
    let mut term = EventTerm::default();
    term.literals = vec![ev_not_sender_literal(addr)];
    let mut filter = EventFilter::default();
    filter.terms = vec![term];
    filter
}

fn tx_include(predicate: transaction_literal::Predicate) -> TransactionLiteral {
    let mut literal = TransactionLiteral::default();
    literal.predicate = Some(predicate);
    literal
}

fn tx_exclude(predicate: transaction_literal::Predicate) -> TransactionLiteral {
    let mut literal = TransactionLiteral::default();
    literal.predicate = Some(predicate);
    literal.negated = true;
    literal
}

fn tx_sender_literal(addr: SuiAddress) -> TransactionLiteral {
    let mut s = SenderFilter::default();
    s.address = Some(addr.to_string());
    tx_include(transaction_literal::Predicate::Sender(s))
}

fn tx_not_sender_literal(addr: SuiAddress) -> TransactionLiteral {
    let mut s = SenderFilter::default();
    s.address = Some(addr.to_string());
    tx_exclude(transaction_literal::Predicate::Sender(s))
}

fn tx_move_call_literal(path: &str) -> TransactionLiteral {
    let mut mc = MoveCallFilter::default();
    mc.function = Some(path.to_string());
    tx_include(transaction_literal::Predicate::MoveCall(mc))
}

fn tx_emit_module_literal(path: &str) -> TransactionLiteral {
    let mut em = EmitModuleFilter::default();
    em.module = Some(path.to_string());
    tx_include(transaction_literal::Predicate::EmitModule(em))
}

fn tx_event_type_literal(path: &str) -> TransactionLiteral {
    let mut et = EventTypeFilter::default();
    et.event_type = Some(path.to_string());
    tx_include(transaction_literal::Predicate::EventType(et))
}

fn tx_event_stream_head_literal(stream_id: ObjectID) -> TransactionLiteral {
    let mut esh = EventStreamHeadFilter::default();
    esh.stream_id = Some(stream_id.to_canonical_string(true));
    tx_include(transaction_literal::Predicate::EventStreamHead(esh))
}

fn tx_package_write_literal() -> TransactionLiteral {
    tx_include(transaction_literal::Predicate::PackageWrite(
        PackageWriteFilter::default(),
    ))
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

fn tx_package_write() -> TransactionFilter {
    tx_filter(vec![tx_package_write_literal()])
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

fn ev_include(predicate: event_literal::Predicate) -> EventLiteral {
    let mut literal = EventLiteral::default();
    literal.predicate = Some(predicate);
    literal
}

fn ev_exclude(predicate: event_literal::Predicate) -> EventLiteral {
    let mut literal = EventLiteral::default();
    literal.predicate = Some(predicate);
    literal.negated = true;
    literal
}

fn ev_sender_literal(addr: SuiAddress) -> EventLiteral {
    let mut s = SenderFilter::default();
    s.address = Some(addr.to_string());
    ev_include(event_literal::Predicate::Sender(s))
}

fn ev_not_sender_literal(addr: SuiAddress) -> EventLiteral {
    let mut s = SenderFilter::default();
    s.address = Some(addr.to_string());
    ev_exclude(event_literal::Predicate::Sender(s))
}

fn ev_event_stream_head_literal(stream_id: ObjectID) -> EventLiteral {
    let mut esh = EventStreamHeadFilter::default();
    esh.stream_id = Some(stream_id.to_canonical_string(true));
    ev_include(event_literal::Predicate::EventStreamHead(esh))
}

fn ev_sender(addr: SuiAddress) -> EventFilter {
    ev_filter(vec![ev_sender_literal(addr)])
}

fn ev_emit_module(path: &str) -> EventFilter {
    let mut em = EmitModuleFilter::default();
    em.module = Some(path.to_string());
    ev_filter(vec![ev_include(event_literal::Predicate::EmitModule(em))])
}

fn ev_event_type(path: &str) -> EventFilter {
    let mut et = EventTypeFilter::default();
    et.event_type = Some(path.to_string());
    ev_filter(vec![ev_include(event_literal::Predicate::EventType(et))])
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

#[sim_test]
async fn test_list_transactions_unfiltered_and_sender_filter() {
    let cluster = new_cluster().await;
    let sender = cluster.get_address_0();
    let tx = transfer_self(&cluster, sender).await;
    let digest = tx_digest(&tx);

    let mut client = new_ledger_client(&cluster).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest", "checkpoint"]));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    assert_transaction_cursors(&resp);
    assert!(
        resp.transactions.len() >= 2,
        "expected genesis plus transfer transactions"
    );
    assert!(
        transaction_digest_set(&resp).contains(&digest),
        "expected to find transfer tx {digest}"
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

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    assert_transaction_cursors(&resp);
    assert!(
        transaction_digest_set(&resp).contains(&digest),
        "sender filter should include transfer tx {digest}"
    );
}

#[sim_test]
async fn test_list_transactions_query_options() {
    let cluster = new_cluster().await;
    let sender = cluster.get_address_0();
    let tx1 = transfer_self(&cluster, sender).await;
    let tx2 = transfer_self(&cluster, sender).await;
    let tx3 = transfer_self(&cluster, sender).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2, &tx3]);

    let mut client = new_ledger_client(&cluster).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(2));
    let response1 = list_transactions_result(&mut client, req).await;
    assert_eq!(response1.transactions.len(), 2);
    assert_item_limit_end(response1.end, response1.end_reason);
    assert_transaction_cursors(&response1);
    let cursor = transaction_end_cursor(&response1, "first response should have end cursor");

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(2, cursor));
    let response2 = list_transactions_result(&mut client, req).await;
    assert_eq!(response2.transactions.len(), 1);
    assert!(response2.end);
    assert_eq!(response2.end_reason, Some(QueryEndReason::CheckpointBound));
    assert_transaction_cursors(&response2);
    let final_cursor = last_transaction_cursor(&response2, "final response should have cursor");

    let first_digests = transaction_digest_set(&response1);
    for digest in transaction_digest_set(&response2) {
        assert!(
            !first_digests.contains(&digest),
            "second page should not overlap first page"
        );
    }

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(2, final_cursor));
    let response3 = list_transactions_result(&mut client, req).await;
    assert!(response3.transactions.is_empty());
    assert!(response3.end);
    assert_eq!(response3.end_reason, Some(QueryEndReason::CheckpointBound));

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_descending(2));
    let reverse1 = list_transactions_result(&mut client, req).await;
    assert_eq!(reverse1.transactions.len(), 2);
    assert_item_limit_end(reverse1.end, reverse1.end_reason);
    let cursor = transaction_end_cursor(&reverse1, "reverse response should have end cursor");

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_descending_before(2, cursor));
    let reverse2 = list_transactions_result(&mut client, req).await;
    assert_eq!(reverse2.transactions.len(), 1);
    assert!(reverse2.end);

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(3));
    let exact = list_transactions_result(&mut client, req).await;
    assert_eq!(exact.transactions.len(), 3);
    assert_item_limit_end(exact.end, exact.end_reason);
    let first_cursor = first_transaction_cursor(&exact, "exact response first cursor");
    let last_cursor = last_transaction_cursor(&exact, "exact response last cursor");

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_between(
        3,
        first_cursor.clone(),
        last_cursor.clone(),
    ));
    let bounded = list_transactions_result(&mut client, req).await;
    assert_eq!(bounded.transactions.len(), 1);
    assert_eq!(bounded.end_reason, Some(QueryEndReason::CursorBound));

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_between_descending(
        3,
        first_cursor,
        last_cursor.clone(),
    ));
    let bounded_desc = list_transactions_result(&mut client, req).await;
    assert_eq!(bounded_desc.transactions.len(), 1);
    assert_eq!(bounded_desc.end_reason, Some(QueryEndReason::CursorBound));

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(3, last_cursor));
    let after_exact = list_transactions_result(&mut client, req).await;
    assert!(after_exact.transactions.is_empty());
    assert!(after_exact.end);
    assert_eq!(
        after_exact.end_reason,
        Some(QueryEndReason::CheckpointBound)
    );
}

#[sim_test]
async fn test_list_transactions_filter_predicates() {
    let cluster = new_cluster().await;
    let sender_a = cluster.get_address_0();
    let sender_b = cluster.get_address_1();
    let sender_c = cluster.get_address_2();

    let (pkg, _) = publish_package(&cluster, sender_a, generic_event_pkg_path()).await;
    let tx_a = call_move(&cluster, sender_a, pkg, "generic_event", "emit_u64").await;
    let tx_b = call_move(&cluster, sender_b, pkg, "generic_event", "emit_address").await;
    let tx_c = transfer_self(&cluster, sender_c).await;
    let digest_a = tx_digest(&tx_a);
    let digest_b = tx_digest(&tx_b);
    let digest_c = tx_digest(&tx_c);

    let client = new_ledger_client(&cluster).await;
    let fetch = |filter: TransactionFilter| {
        let mut client = client.clone();
        async move {
            let mut req = ListTransactionsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["digest"]));
            req.filter = Some(filter);
            req.options = Some(query_options(100));
            list_transactions_result(&mut client, req).await
        }
    };

    let resp = fetch(tx_or(vec![
        vec![tx_sender_literal(sender_a)],
        vec![tx_sender_literal(sender_b)],
    ]))
    .await;
    let digests = transaction_digest_set(&resp);
    assert!(
        digests.contains(&digest_a) && digests.contains(&digest_b),
        "sender OR should include A and B calls, got {digests:?}"
    );
    assert!(
        !digests.contains(&digest_c),
        "sender OR should not include C transfer"
    );

    let pkg_path = pkg.to_canonical_string(true);
    let module_path = format!("{pkg_path}::generic_event");
    let emit_u64_path = format!("{module_path}::emit_u64");

    for filter in [tx_move_call(&pkg_path), tx_move_call(&module_path)] {
        let digests = transaction_digest_set(&fetch(filter).await);
        assert!(
            digests.contains(&digest_a) && digests.contains(&digest_b),
            "package/module move-call prefixes should match both calls, got {digests:?}"
        );
        assert!(
            !digests.contains(&digest_c),
            "move-call prefix should not include C transfer"
        );
    }

    let digests = transaction_digest_set(&fetch(tx_move_call(&emit_u64_path)).await);
    assert!(
        digests.contains(&digest_a) && !digests.contains(&digest_b),
        "function-level move-call filter should match only emit_u64, got {digests:?}"
    );

    for filter in [tx_emit_module(&pkg_path), tx_emit_module(&module_path)] {
        let digests = transaction_digest_set(&fetch(filter).await);
        assert!(
            digests.contains(&digest_a) && digests.contains(&digest_b),
            "tx emit_module prefixes should match event-emitting txs, got {digests:?}"
        );
        assert!(
            !digests.contains(&digest_c),
            "tx emit_module filter should not include C transfer"
        );
    }

    let u64_event_type = format!("{module_path}::GenericEvent<u64>");
    let digests = transaction_digest_set(&fetch(tx_event_type(&u64_event_type)).await);
    assert!(
        digests.contains(&digest_a) && !digests.contains(&digest_b),
        "tx event_type filter should match only GenericEvent<u64>, got {digests:?}"
    );
}

#[sim_test]
async fn test_list_package_write_filter() {
    let cluster = new_cluster().await;
    let sender = cluster.get_address_0();

    // A publish writes a Move package; the transfer writes none. Each helper
    // waits for its transaction to be sealed into a checkpoint before returning
    // (execute_transaction_and_wait_for_checkpoint), so the publish's checkpoint
    // is finalized before the transfer is submitted — they land in distinct
    // checkpoints, letting us test checkpoint-level exclusion deterministically.
    let (_pkg, publish_tx) = publish_package(&cluster, sender, emit_test_event_pkg_path()).await;
    let transfer_tx = transfer_self(&cluster, sender).await;
    let publish_digest = tx_digest(&publish_tx);
    let transfer_digest = tx_digest(&transfer_tx);
    let publish_cp = tx_checkpoint(&publish_tx);
    let transfer_cp = tx_checkpoint(&transfer_tx);
    assert_ne!(
        publish_cp, transfer_cp,
        "publish and transfer should occupy distinct checkpoints"
    );

    let mut client = new_ledger_client(&cluster).await;

    // Transaction-level: the publish matches the filter, the transfer does not.
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_package_write());
    req.options = Some(query_options(100));
    let digests = transaction_digest_set(&list_transactions_result(&mut client, req).await);
    assert!(
        digests.contains(&publish_digest),
        "package_write filter should include the publish tx, got {digests:?}"
    );
    assert!(
        !digests.contains(&transfer_digest),
        "package_write filter should exclude the transfer tx, got {digests:?}"
    );

    // Checkpoint-level: the publish's checkpoint matches, the transfer-only
    // checkpoint does not.
    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(tx_package_write());
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    let seqs: HashSet<u64> = resp.checkpoints.iter().map(checkpoint_sequence).collect();
    assert!(
        seqs.contains(&publish_cp),
        "package_write checkpoint filter should include the publish checkpoint {publish_cp}, got {seqs:?}"
    );
    assert!(
        !seqs.contains(&transfer_cp),
        "package_write checkpoint filter should exclude the transfer-only checkpoint {transfer_cp}, got {seqs:?}"
    );
}

#[sim_test]
async fn test_list_transactions_combinators_and_affected_filters() {
    let cluster = new_cluster().await;
    let sender_a = cluster.get_address_0();
    let sender_b = cluster.get_address_1();

    let (pkg, _) = publish_package(&cluster, sender_a, emit_test_event_pkg_path()).await;
    let tx_a_call = call_move(
        &cluster,
        sender_a,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;
    let tx_a_transfer = transfer_self(&cluster, sender_a).await;
    let tx_b_call = call_move(
        &cluster,
        sender_b,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;
    let digest_a_call = tx_digest(&tx_a_call);
    let digest_a_transfer = tx_digest(&tx_a_transfer);
    let digest_b_call = tx_digest(&tx_b_call);

    let mut client = new_ledger_client(&cluster).await;
    let move_call_path = format!(
        "{}::emit_test_event::emit_test_event",
        pkg.to_canonical_string(true)
    );

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_and(vec![
        tx_sender(sender_a),
        tx_move_call(&move_call_path),
    ]));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    let digests = transaction_digest_set(&resp);
    assert!(digests.contains(&digest_a_call), "expected A+call match");
    assert!(
        !digests.contains(&digest_a_transfer),
        "A transfer should not match move-call predicate"
    );
    assert!(
        !digests.contains(&digest_b_call),
        "B call should not match sender predicate"
    );

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_filter(vec![
        tx_sender_literal(sender_a),
        tx_not_sender_literal(sender_b),
    ]));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    let digests = transaction_digest_set(&resp);
    assert!(digests.contains(&digest_a_call));
    assert!(digests.contains(&digest_a_transfer));
    assert!(!digests.contains(&digest_b_call));

    let (transfer_to_b, affected_gas_id) = split_transfer(&cluster, sender_a, sender_b).await;
    let transfer_to_b_digest = tx_digest(&transfer_to_b);

    let mut affected_address = AffectedAddressFilter::default();
    affected_address.address = Some(sender_b.to_string());
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_filter(vec![tx_include(
        transaction_literal::Predicate::AffectedAddress(affected_address),
    )]));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    assert!(
        transaction_digest_set(&resp).contains(&transfer_to_b_digest),
        "recipient affected-address filter should include split transfer"
    );

    let mut affected_object = AffectedObjectFilter::default();
    affected_object.object_id = Some(affected_gas_id.to_canonical_string(true));
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_filter(vec![tx_include(
        transaction_literal::Predicate::AffectedObject(affected_object),
    )]));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    assert!(
        transaction_digest_set(&resp).contains(&transfer_to_b_digest),
        "affected-object filter should include split transfer"
    );
}

#[sim_test]
async fn test_list_events_unfiltered_and_emit_module_filter() {
    let cluster = new_cluster().await;
    let sender = cluster.get_address_0();
    let (pkg, _) = publish_package(&cluster, sender, emit_test_event_pkg_path()).await;
    let event_tx = call_move(&cluster, sender, pkg, "emit_test_event", "emit_test_event").await;
    let event_digest = tx_digest(&event_tx);

    let mut client = new_ledger_client(&cluster).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    assert_event_cursors(&resp);
    assert!(
        event_digest_set(&resp).contains(&event_digest),
        "unfiltered events should include emitted event"
    );
    let event_type = resp
        .events
        .iter()
        .find(|event| event_transaction_digest(event).as_deref() == Some(event_digest.as_str()))
        .and_then(|event| event.event.as_ref())
        .and_then(|event| event.event_type.as_deref())
        .expect("emitted event type should be present");
    assert!(event_type.contains("emit_test_event::TestEvent"));

    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    assert_event_cursors(&resp);
    assert!(
        event_digest_set(&resp).contains(&event_digest),
        "emit_module filter should include emitted event"
    );
    for event in &resp.events {
        let event_type = event
            .event
            .as_ref()
            .and_then(|event| event.event_type.as_deref())
            .expect("event_type should be present");
        assert!(
            event_type.contains("emit_test_event"),
            "all events should be from emit_test_event module, got {event_type}"
        );
    }
}

#[sim_test]
async fn test_list_events_query_options_multi_event_tx() {
    let cluster = new_cluster().await;
    let sender = cluster.get_address_0();
    let (pkg, _) = publish_package(&cluster, sender, emit_test_event_pkg_path()).await;

    let tx1 = call_emit_many(&cluster, sender, pkg, 5).await;
    let tx2 = call_emit_many(&cluster, sender, pkg, 3).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2]);

    let client = new_ledger_client(&cluster).await;
    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));

    let response = |filter: EventFilter, cursor: Option<Bytes>| {
        let mut client = client.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(event_type_and_position_mask());
            req.start_checkpoint = Some(start);
            req.end_checkpoint = Some(end);
            req.filter = Some(filter);
            req.options = Some(query_options_maybe_after(3, cursor));
            list_events_result(&mut client, req).await
        }
    };

    let r1 = response(ev_emit_module(&module), None).await;
    assert_eq!(r1.events.len(), 3);
    assert_item_limit_end(r1.end, r1.end_reason);
    assert_event_cursors(&r1);
    let r1_cursor = event_end_cursor(&r1, "response 1 should have end cursor");

    let r2 = response(ev_emit_module(&module), Some(r1_cursor)).await;
    assert_eq!(r2.events.len(), 3);
    assert_item_limit_end(r2.end, r2.end_reason);
    assert_event_cursors(&r2);
    let r2_cursor = event_end_cursor(&r2, "response 2 should have end cursor");

    let r3 = response(ev_emit_module(&module), Some(r2_cursor)).await;
    assert_eq!(r3.events.len(), 2);
    assert!(r3.end);
    assert_event_cursors(&r3);

    let mut all_cursors: Vec<_> = r1
        .events
        .iter()
        .chain(r2.events.iter())
        .chain(r3.events.iter())
        .map(|event| event.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .collect();
    let total = all_cursors.len();
    all_cursors.sort();
    all_cursors.dedup();
    assert_eq!(all_cursors.len(), total, "no duplicate event cursors");
    assert_eq!(total, 8, "expected 8 events");

    let mut client_for_exact = client.clone();
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options(8));
    let exact = list_events_result(&mut client_for_exact, req).await;
    assert_eq!(exact.events.len(), 8);
    assert_item_limit_end(exact.end, exact.end_reason);
    let first_cursor = first_event_cursor(&exact, "exact first event cursor");
    let last_cursor = last_event_cursor(&exact, "exact last event cursor");

    let mut client_for_bounds = client.clone();
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options_between(8, first_cursor, last_cursor.clone()));
    let bounded = list_events_result(&mut client_for_bounds, req).await;
    assert_eq!(bounded.events.len(), 6);
    assert_eq!(bounded.end_reason, Some(QueryEndReason::CursorBound));

    let reverse_response = |cursor: Option<Bytes>| {
        let mut client = client.clone();
        let module = module.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(event_type_and_position_mask());
            req.start_checkpoint = Some(start);
            req.end_checkpoint = Some(end);
            req.filter = Some(ev_emit_module(&module));
            req.options = Some(query_options_descending_maybe_before(3, cursor));
            list_events_result(&mut client, req).await
        }
    };

    let rp1 = reverse_response(None).await;
    assert_eq!(rp1.events.len(), 3);
    assert_item_limit_end(rp1.end, rp1.end_reason);
    let rp1_cursor = event_end_cursor(&rp1, "reverse response 1 should have end cursor");
    let rp2 = reverse_response(Some(rp1_cursor)).await;
    assert_eq!(rp2.events.len(), 3);
    assert_item_limit_end(rp2.end, rp2.end_reason);
    let rp2_cursor = event_end_cursor(&rp2, "reverse response 2 should have end cursor");
    let rp3 = reverse_response(Some(rp2_cursor)).await;
    assert_eq!(rp3.events.len(), 2);
    assert!(rp3.end);

    let reverse_keys: Vec<_> = rp1
        .events
        .iter()
        .chain(rp2.events.iter())
        .chain(rp3.events.iter())
        .map(|event| (event_transaction_digest(event), event_index_of(event)))
        .collect();
    let mut deduped = reverse_keys.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(deduped.len(), reverse_keys.len(), "no reverse duplicates");

    let mut client_after_exact = client.clone();
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options_after(8, last_cursor));
    let after_exact = list_events_result(&mut client_after_exact, req).await;
    assert!(after_exact.events.is_empty());
    assert!(after_exact.end);
    assert_eq!(
        after_exact.end_reason,
        Some(QueryEndReason::CheckpointBound)
    );
}

#[sim_test]
async fn test_list_events_filter_predicates() {
    let cluster = new_cluster().await;
    let sender_a = cluster.get_address_0();
    let sender_b = cluster.get_address_1();
    let sender_c = cluster.get_address_2();

    let (generic_pkg, _) = publish_package(&cluster, sender_a, generic_event_pkg_path()).await;
    let tx_u64 = call_move(&cluster, sender_a, generic_pkg, "generic_event", "emit_u64").await;
    let tx_addr_b = call_move(
        &cluster,
        sender_b,
        generic_pkg,
        "generic_event",
        "emit_address",
    )
    .await;
    let tx_addr_c = call_move(
        &cluster,
        sender_c,
        generic_pkg,
        "generic_event",
        "emit_address",
    )
    .await;
    let digest_u64 = tx_digest(&tx_u64);
    let digest_addr_b = tx_digest(&tx_addr_b);
    let digest_addr_c = tx_digest(&tx_addr_c);

    let client = new_ledger_client(&cluster).await;
    let fetch = |filter: EventFilter| {
        let mut client = client.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(event_type_and_position_mask());
            req.filter = Some(filter);
            req.options = Some(query_options(100));
            list_events_result(&mut client, req).await
        }
    };

    let resp = fetch(ev_or(vec![
        vec![ev_sender_literal(sender_a)],
        vec![ev_sender_literal(sender_b)],
    ]))
    .await;
    let digests = event_digest_set(&resp);
    assert!(
        digests.contains(&digest_u64) && digests.contains(&digest_addr_b),
        "event sender OR should include A and B events, got {digests:?}"
    );
    assert!(
        !digests.contains(&digest_addr_c),
        "event sender OR should not include C event"
    );

    let pkg_hex = generic_pkg.to_canonical_string(true);
    let module = format!("{pkg_hex}::generic_event");
    let name = format!("{module}::GenericEvent");
    let resp = fetch(ev_event_type(&name)).await;
    let digests = event_digest_set(&resp);
    assert!(
        digests.contains(&digest_u64)
            && digests.contains(&digest_addr_b)
            && digests.contains(&digest_addr_c),
        "name-level event type should match all generic events, got {digests:?}"
    );

    let u64_type = format!("{module}::GenericEvent<u64>");
    let resp = fetch(ev_event_type(&u64_type)).await;
    let digests = event_digest_set(&resp);
    assert!(
        digests.contains(&digest_u64) && !digests.contains(&digest_addr_b),
        "instantiated event type should match only u64 event, got {digests:?}"
    );

    let resp = fetch(ev_event_type(&module)).await;
    let digests = event_digest_set(&resp);
    assert!(
        digests.contains(&digest_u64) && digests.contains(&digest_addr_b),
        "module-level event type should match generic events, got {digests:?}"
    );

    let (auth_pkg, _) = publish_package(&cluster, sender_a, authenticated_event_pkg_path()).await;
    let auth_tx = call_move(
        &cluster,
        sender_a,
        auth_pkg,
        "authenticated_event",
        "emit_both",
    )
    .await;
    let auth_digest = tx_digest(&auth_tx);

    let resp = fetch(ev_event_stream_head(auth_pkg)).await;
    assert_eq!(
        resp.events.len(),
        1,
        "event_stream_head should return only authenticated event"
    );
    assert_eq!(
        event_transaction_digest(&resp.events[0]).as_deref(),
        Some(auth_digest.as_str())
    );
    let event_type = resp.events[0]
        .event
        .as_ref()
        .and_then(|event| event.event_type.as_deref())
        .expect("authenticated event type should be present");
    assert!(event_type.contains("authenticated_event::AuthenticatedEvent"));

    let mut tx_client = client.clone();
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_event_stream_head(auth_pkg));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut tx_client, req).await;
    assert!(
        transaction_digest_set(&resp).contains(&auth_digest),
        "tx event_stream_head should include authenticated event transaction"
    );
}

#[sim_test]
async fn test_list_events_combinators() {
    let cluster = new_cluster().await;
    let sender_a = cluster.get_address_0();
    let sender_b = cluster.get_address_1();

    let (pkg, _) = publish_package(&cluster, sender_a, emit_test_event_pkg_path()).await;
    let tx_a = call_move(
        &cluster,
        sender_a,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;
    let tx_b = call_move(
        &cluster,
        sender_b,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;
    let digest_a = tx_digest(&tx_a);
    let digest_b = tx_digest(&tx_b);

    let mut client = new_ledger_client(&cluster).await;
    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.filter = Some(ev_and(vec![ev_sender(sender_a), ev_emit_module(&module)]));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    let digests = event_digest_set(&resp);
    assert!(
        digests.contains(&digest_a),
        "A event should match AND filter"
    );
    assert!(
        !digests.contains(&digest_b),
        "B event should be excluded by sender predicate"
    );

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.filter = Some(ev_filter(vec![
        ev_sender_literal(sender_a),
        ev_not_sender_literal(sender_b),
    ]));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    let digests = event_digest_set(&resp);
    assert!(
        digests.contains(&digest_a),
        "A event should match NOT filter"
    );
    assert!(
        !digests.contains(&digest_b),
        "B event should be excluded by Not(Sender=B)"
    );
}

#[sim_test]
async fn test_list_events_unanchored_negation() {
    let cluster = new_cluster().await;
    let sender_a = cluster.get_address_0();
    let sender_b = cluster.get_address_1();

    let (pkg, _) = publish_package(&cluster, sender_a, emit_test_event_pkg_path()).await;
    let tx_a = call_move(
        &cluster,
        sender_a,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;
    let tx_b = call_move(
        &cluster,
        sender_b,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;
    let digest_a = tx_digest(&tx_a);
    let digest_b = tx_digest(&tx_b);

    let mut client = new_ledger_client(&cluster).await;

    // Single-term exclude-only filter: `NOT sender = B` must return A's event
    // (and any other event whose sender is not B), validating that the
    // synthesized EventExtant include actually anchors the term so the driver
    // walks the event space rather than rejecting the filter.
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.filter = Some(ev_not_sender_only_filter(sender_b));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    let digests = event_digest_set(&resp);
    assert!(
        digests.contains(&digest_a),
        "A event should match unanchored NOT(Sender=B)"
    );
    assert!(
        !digests.contains(&digest_b),
        "B event should be excluded by unanchored NOT(Sender=B)"
    );
    assert!(resp.end);

    // Symmetric case: `NOT sender = A` returns B's event.
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.filter = Some(ev_not_sender_only_filter(sender_a));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    let digests = event_digest_set(&resp);
    assert!(
        digests.contains(&digest_b),
        "B event should match unanchored NOT(Sender=A)"
    );
    assert!(
        !digests.contains(&digest_a),
        "A event should be excluded by unanchored NOT(Sender=A)"
    );

    // DNF with two unanchored terms `NOT sender = A OR NOT sender = B` —
    // every emitted event satisfies at least one branch, so both digests
    // come back. Exercises the dedup path: the synthetic EventExtant leaf
    // is shared across both terms and must only be scanned once.
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.filter = Some(ev_or(vec![
        vec![ev_not_sender_literal(sender_a)],
        vec![ev_not_sender_literal(sender_b)],
    ]));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    let digests = event_digest_set(&resp);
    assert!(digests.contains(&digest_a));
    assert!(digests.contains(&digest_b));
}

#[sim_test]
async fn test_list_transactions_unanchored_negation() {
    let cluster = new_cluster().await;
    let sender_a = cluster.get_address_0();
    let sender_b = cluster.get_address_1();

    let tx_a = transfer_self(&cluster, sender_a).await;
    let tx_b = transfer_self(&cluster, sender_b).await;
    let digest_a = tx_digest(&tx_a);
    let digest_b = tx_digest(&tx_b);
    let (start, end) = checkpoint_range(&[&tx_a, &tx_b]);

    let mut client = new_ledger_client(&cluster).await;

    // Single-term exclude-only filter: `NOT sender = B` must return A's tx
    // and every other tx in range (including system transactions), validating
    // that the synthesized TxUniverse include anchors the term so the driver
    // walks the dense tx space rather than rejecting the filter.
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_not_sender_only_filter(sender_b));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    assert_transaction_cursors(&resp);
    let digests = transaction_digest_set(&resp);
    assert!(
        digests.contains(&digest_a),
        "A tx should match unanchored NOT(Sender=B)"
    );
    assert!(
        !digests.contains(&digest_b),
        "B tx should be excluded by unanchored NOT(Sender=B)"
    );
    assert!(
        resp.transactions.len() >= 2,
        "complement includes system transactions in range"
    );
    assert!(resp.end);
    let full_complement = digests;

    // Symmetric case: `NOT sender = A` returns B's tx.
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_not_sender_only_filter(sender_a));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    let digests = transaction_digest_set(&resp);
    assert!(
        digests.contains(&digest_b),
        "B tx should match unanchored NOT(Sender=A)"
    );
    assert!(
        !digests.contains(&digest_a),
        "A tx should be excluded by unanchored NOT(Sender=A)"
    );

    // DNF with two unanchored terms `NOT sender = A OR NOT sender = B` —
    // every tx satisfies at least one branch, so both digests return.
    // Confirms a multi-term exclude-only DNF survives the full stack.
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_or(vec![
        vec![tx_not_sender_literal(sender_a)],
        vec![tx_not_sender_literal(sender_b)],
    ]));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    let digests = transaction_digest_set(&resp);
    assert!(digests.contains(&digest_a));
    assert!(digests.contains(&digest_b));

    // Pagination across the dense complement: small pages with cursor resume
    // must cover exactly the single-shot result with no overlap.
    let mut paged: HashSet<String> = HashSet::new();
    let mut after: Option<Bytes> = None;
    loop {
        let mut req = ListTransactionsRequest::default();
        req.read_mask = Some(FieldMask::from_paths(["digest"]));
        req.start_checkpoint = Some(start);
        req.end_checkpoint = Some(end);
        req.filter = Some(tx_not_sender_only_filter(sender_b));
        req.options = Some(query_options_maybe_after(2, after.clone()));
        let resp = list_transactions_result(&mut client, req).await;
        for digest in transaction_digest_set(&resp) {
            assert!(paged.insert(digest), "pages must not overlap");
        }
        if resp.end_reason != Some(QueryEndReason::ItemLimit) {
            break;
        }
        after = Some(transaction_end_cursor(
            &resp,
            "item-limited page should carry an end cursor",
        ));
    }
    assert_eq!(
        paged, full_complement,
        "paged union must equal the single-shot complement"
    );
}

#[sim_test]
async fn test_list_checkpoints_unanchored_negation() {
    let cluster = new_cluster().await;
    let sender_a = cluster.get_address_0();
    let sender_b = cluster.get_address_1();

    let tx_a = transfer_self(&cluster, sender_a).await;
    let tx_b = transfer_self(&cluster, sender_b).await;
    let cp_a = tx_checkpoint(&tx_a);
    let cp_b = tx_checkpoint(&tx_b);
    let (start, end) = checkpoint_range(&[&tx_a, &tx_b]);

    let mut client = new_ledger_client(&cluster).await;

    // Checkpoint filters reuse the tx filter machinery: a checkpoint matches
    // when it contains at least one matching tx. `NOT sender = A` matches the
    // system transactions in every checkpoint, so the unanchored filter must
    // return the same checkpoint set as an unfiltered scan of the range —
    // pinning the dense complement through the tx→checkpoint mapping.
    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.options = Some(query_options(100));
    let unfiltered: Vec<u64> = list_checkpoints_result(&mut client, req)
        .await
        .checkpoints
        .iter()
        .map(checkpoint_sequence)
        .collect();

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_not_sender_only_filter(sender_a));
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert_checkpoint_cursors(&resp);
    let filtered: Vec<u64> = resp.checkpoints.iter().map(checkpoint_sequence).collect();
    assert_eq!(
        filtered, unfiltered,
        "every checkpoint contains a non-A system tx, so the complement covers the range"
    );
    assert!(filtered.contains(&cp_a));
    assert!(filtered.contains(&cp_b));
    assert!(resp.end);
}

#[sim_test]
async fn test_list_filter_edge_cases_and_limit_caps() {
    let cluster = new_cluster().await;
    let sender = cluster.get_address_0();
    transfer_self(&cluster, sender).await;

    let mut client = new_ledger_client(&cluster).await;
    let beyond_tip = latest_checkpoint_sequence(&cluster).await + DEFAULT_CHECKPOINT_RANGE_END;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(beyond_tip);
    req.options = Some(query_options(10));
    let resp = list_transactions_result(&mut client, req).await;
    assert!(resp.transactions.is_empty(), "no txs beyond indexed range");
    assert!(resp.end);
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.start_checkpoint = Some(beyond_tip);
    req.options = Some(query_options(10));
    let resp = list_events_result(&mut client, req).await;
    assert!(resp.events.is_empty(), "no events beyond indexed range");
    assert!(resp.end);
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(beyond_tip);
    req.options = Some(query_options(10));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(
        resp.checkpoints.is_empty(),
        "no checkpoints beyond indexed range"
    );
    assert!(resp.end);
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));

    let never_sender: SuiAddress =
        "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
            .parse()
            .unwrap();
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_sender(never_sender));
    req.options = Some(query_options(10));
    let resp = list_transactions_result(&mut client, req).await;
    assert!(resp.transactions.is_empty(), "no-match tx filter");
    assert!(resp.end);

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.filter = Some(ev_sender(never_sender));
    req.options = Some(query_options(10));
    let resp = list_events_result(&mut client, req).await;
    assert!(resp.events.is_empty(), "no-match event filter");
    assert!(resp.end);

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(tx_sender(never_sender));
    req.options = Some(query_options(10));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(resp.checkpoints.is_empty(), "no-match checkpoint filter");
    assert!(resp.end);

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(tx_move_call("0x1::a::b::c"));
    req.options = Some(query_options(10));
    expect_invalid_list_transactions(&mut client, req).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(ev_event_type("0x1<u64>"));
    req.options = Some(query_options(10));
    expect_invalid_list_events(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.options = Some(query_options(10));
    assert!(list_transactions_result(&mut client, req).await.end);

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.options = Some(query_options(10));
    assert!(list_transactions_result(&mut client, req).await.end);

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(10);
    req.end_checkpoint = Some(9);
    req.options = Some(query_options(10));
    expect_invalid_list_transactions(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(beyond_tip);
    req.end_checkpoint = Some(beyond_tip + 1);
    req.options = Some(query_options(10));
    let resp = list_transactions_result(&mut client, req).await;
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    let mut bad_options = query_options(10);
    bad_options.after = Some(Bytes::from_static(b"short"));
    req.options = Some(bad_options);
    expect_invalid_list_transactions(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    let mut bad_options = query_options(10);
    bad_options.ordering = Some(99);
    req.options = Some(bad_options);
    expect_invalid_list_transactions(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(TransactionFilter::default());
    req.options = Some(query_options(10));
    expect_invalid_list_transactions(&mut client, req).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(EventFilter::default());
    req.options = Some(query_options(10));
    expect_invalid_list_events(&mut client, req).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.options = Some(query_options(10));
    assert!(list_checkpoints_result(&mut client, req).await.end);

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
    req.read_mask = Some(event_type_and_position_mask());
    req.options = Some(query_options(oversized));
    list_events_result(&mut client, req).await;
}

#[sim_test]
async fn test_list_checkpoints_filters_and_ordering() {
    let cluster = new_cluster().await;
    let sender_a = cluster.get_address_0();
    let sender_b = cluster.get_address_1();
    let sender_c = cluster.get_address_2();

    let tx_a = transfer_self(&cluster, sender_a).await;
    let tx_b = transfer_self(&cluster, sender_b).await;
    let tx_c = transfer_self(&cluster, sender_c).await;
    let cp_a = tx_checkpoint(&tx_a);
    let cp_b = tx_checkpoint(&tx_b);
    let cp_c = tx_checkpoint(&tx_c);

    let mut client = new_ledger_client(&cluster).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert_checkpoint_cursors(&resp);
    assert!(
        resp.checkpoints.len() >= 4,
        "expected genesis checkpoint plus created checkpoints"
    );
    for window in resp.checkpoints.windows(2) {
        let a = checkpoint_sequence(&window[0]);
        let b = checkpoint_sequence(&window[1]);
        assert!(
            a < b,
            "checkpoints should be strictly increasing: {a} >= {b}"
        );
    }

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(tx_sender(sender_a));
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert_checkpoint_cursors(&resp);
    let seqs: HashSet<u64> = resp.checkpoints.iter().map(checkpoint_sequence).collect();
    assert!(
        seqs.contains(&cp_a),
        "sender_a should match checkpoint {cp_a}, got {seqs:?}"
    );

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(tx_or(vec![
        vec![tx_sender_literal(sender_a)],
        vec![tx_sender_literal(sender_b)],
    ]));
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert_checkpoint_cursors(&resp);
    let seqs: HashSet<u64> = resp.checkpoints.iter().map(checkpoint_sequence).collect();
    assert!(
        seqs.contains(&cp_a) && seqs.contains(&cp_b),
        "OR filter should include checkpoints for A and B, got {seqs:?}"
    );
    assert!(
        !seqs.contains(&cp_c),
        "OR filter should exclude checkpoint for C"
    );

    let (pkg, _) = publish_package(&cluster, sender_a, emit_test_event_pkg_path()).await;
    let tx_a_call = call_move(
        &cluster,
        sender_a,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;
    let tx_a_transfer = transfer_self(&cluster, sender_a).await;
    let tx_b_call = call_move(
        &cluster,
        sender_b,
        pkg,
        "emit_test_event",
        "emit_test_event",
    )
    .await;
    let cp_a_call = tx_checkpoint(&tx_a_call);
    let cp_a_transfer = tx_checkpoint(&tx_a_transfer);
    let cp_b_call = tx_checkpoint(&tx_b_call);

    let move_call_path = format!(
        "{}::emit_test_event::emit_test_event",
        pkg.to_canonical_string(true)
    );
    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(tx_and(vec![
        tx_sender(sender_a),
        tx_move_call(&move_call_path),
    ]));
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert_checkpoint_cursors(&resp);
    let seqs: HashSet<u64> = resp.checkpoints.iter().map(checkpoint_sequence).collect();
    assert!(
        seqs.contains(&cp_a_call),
        "expected checkpoint containing A+call to match"
    );
    assert!(
        !seqs.contains(&cp_a_transfer),
        "checkpoint containing A transfer must not match move-call predicate"
    );
    assert!(
        !seqs.contains(&cp_b_call),
        "checkpoint containing B call must not match sender predicate"
    );
}

#[sim_test]
async fn test_list_checkpoints_query_options() {
    let cluster = new_cluster().await;
    let sender = cluster.get_address_0();
    let tx1 = transfer_self(&cluster, sender).await;
    let tx2 = transfer_self(&cluster, sender).await;
    let tx3 = transfer_self(&cluster, sender).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2, &tx3]);

    let mut client = new_ledger_client(&cluster).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(2));
    let response1 = list_checkpoints_result(&mut client, req).await;
    assert_eq!(response1.checkpoints.len(), 2);
    assert_item_limit_end(response1.end, response1.end_reason);
    assert_checkpoint_cursors(&response1);
    let cursor = checkpoint_end_cursor(&response1, "first checkpoint response cursor");
    let response1_seqs: Vec<_> = response1
        .checkpoints
        .iter()
        .map(checkpoint_sequence)
        .collect();

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(2, cursor));
    let response2 = list_checkpoints_result(&mut client, req).await;
    assert_eq!(response2.checkpoints.len(), 1);
    assert!(response2.end);
    assert_eq!(response2.end_reason, Some(QueryEndReason::CheckpointBound));
    assert_checkpoint_cursors(&response2);
    let response2_seqs: Vec<_> = response2
        .checkpoints
        .iter()
        .map(checkpoint_sequence)
        .collect();
    for seq in &response2_seqs {
        assert!(!response1_seqs.contains(seq));
        assert!(*seq > *response1_seqs.last().unwrap());
    }

    let final_cursor = last_checkpoint_cursor(&response2, "final checkpoint response cursor");
    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(2, final_cursor));
    let response3 = list_checkpoints_result(&mut client, req).await;
    assert!(response3.checkpoints.is_empty());
    assert!(response3.end);
    assert_eq!(response3.end_reason, Some(QueryEndReason::CursorBound));

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_descending(2));
    let reverse1 = list_checkpoints_result(&mut client, req).await;
    assert_eq!(reverse1.checkpoints.len(), 2);
    assert_item_limit_end(reverse1.end, reverse1.end_reason);
    let reverse1_seqs: Vec<_> = reverse1
        .checkpoints
        .iter()
        .map(checkpoint_sequence)
        .collect();
    assert!(
        reverse1_seqs.windows(2).all(|pair| pair[0] > pair[1]),
        "reverse checkpoints should be descending"
    );
    let cursor = checkpoint_end_cursor(&reverse1, "reverse checkpoint cursor");

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_descending_before(2, cursor));
    let reverse2 = list_checkpoints_result(&mut client, req).await;
    assert_eq!(reverse2.checkpoints.len(), 1);
    assert!(reverse2.end);
    let reverse2_seqs: Vec<_> = reverse2
        .checkpoints
        .iter()
        .map(checkpoint_sequence)
        .collect();
    assert!(
        reverse2_seqs
            .iter()
            .all(|seq| *seq < *reverse1_seqs.last().unwrap()),
        "second reverse page should resume before first page"
    );

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(3));
    let exact = list_checkpoints_result(&mut client, req).await;
    assert_eq!(exact.checkpoints.len(), 3);
    assert_item_limit_end(exact.end, exact.end_reason);
    let first_cursor = first_checkpoint_cursor(&exact, "exact first checkpoint cursor");
    let last_cursor = last_checkpoint_cursor(&exact, "exact last checkpoint cursor");

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_between(
        3,
        first_cursor.clone(),
        last_cursor.clone(),
    ));
    let bounded = list_checkpoints_result(&mut client, req).await;
    assert_eq!(bounded.checkpoints.len(), 1);
    assert_eq!(bounded.end_reason, Some(QueryEndReason::CursorBound));

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_between_descending(
        3,
        first_cursor,
        last_cursor.clone(),
    ));
    let bounded_desc = list_checkpoints_result(&mut client, req).await;
    assert_eq!(bounded_desc.checkpoints.len(), 1);
    assert_eq!(bounded_desc.end_reason, Some(QueryEndReason::CursorBound));

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(3, last_cursor));
    let after_exact = list_checkpoints_result(&mut client, req).await;
    assert!(after_exact.checkpoints.is_empty());
    assert!(after_exact.end);
    assert_eq!(after_exact.end_reason, Some(QueryEndReason::CursorBound));
}

#[sim_test]
async fn test_list_checkpoints_read_masks_and_empty_range() {
    let cluster = new_cluster().await;
    let sender = cluster.get_address_0();

    let mut client = new_ledger_client(&cluster).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint =
        Some(latest_checkpoint_sequence(&cluster).await + DEFAULT_CHECKPOINT_RANGE_END);
    req.options = Some(query_options(10));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(resp.checkpoints.is_empty());
    assert!(resp.end);
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));

    let tx1 = transfer_self(&cluster, sender).await;
    let tx2 = transfer_self(&cluster, sender).await;
    let expected_digests = [tx_digest(&tx1), tx_digest(&tx2)];
    let (start, end) = checkpoint_range(&[&tx1, &tx2]);

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths([
        "sequence_number",
        "transactions.digest",
    ]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;

    let returned_digests: HashSet<String> = resp
        .checkpoints
        .iter()
        .flat_map(|item| {
            item.checkpoint
                .as_ref()
                .expect("checkpoint populated")
                .transactions
                .iter()
                .filter_map(|tx| tx.digest.clone())
        })
        .collect();
    for expected in &expected_digests {
        assert!(
            returned_digests.contains(expected),
            "expected digest {expected} in transactions[].digest, got {returned_digests:?}"
        );
    }

    let gas = gas_object(&cluster, sender).await;
    let gas_id = gas.0.to_canonical_string(true);
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender, None);
    let tx = execute_programmable(&cluster, sender, gas, builder).await;
    let cp = tx_checkpoint(&tx);

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths([
        "sequence_number",
        "transactions.digest",
        "objects.objects.object_id",
        "objects.objects.version",
    ]));
    req.start_checkpoint = Some(cp);
    req.end_checkpoint = Some(cp + 1);
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;

    let saw_gas_object = resp.checkpoints.iter().any(|item| {
        item.checkpoint
            .as_ref()
            .and_then(|checkpoint| checkpoint.objects.as_ref())
            .is_some_and(|objects| {
                objects.objects.iter().any(|object| {
                    object
                        .object_id
                        .as_ref()
                        .is_some_and(|object_id| object_id == &gas_id)
                })
            })
    });
    assert!(
        saw_gas_object,
        "expected gas object {gas_id} in checkpoint objects[]"
    );

    let any_transactions = resp.checkpoints.iter().any(|item| {
        item.checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.transactions.iter().any(|tx| tx.digest.is_some()))
    });
    assert!(
        any_transactions,
        "expected transactions[].digest populated with objects read mask"
    );
}

// list_checkpoints dedupes cp_seq, so an emitted checkpoint is proven complete:
// its item watermark must claim its OWN sequence number as the covered boundary
// (`checkpoint` == sequence_number, in either ordering), not sequence_number ∓ 1.
// This pins the item path onto `advance_checkpoint_boundary`; the previous
// (buggy) `advance_boundary_excluding_cp` path under-claimed by one. Also asserts
// the wire-documented monotonicity.
#[sim_test]
async fn test_list_checkpoints_item_watermark_boundary() {
    let cluster = new_cluster().await;
    let sender = cluster.get_address_0();
    let tx1 = transfer_self(&cluster, sender).await;
    let tx2 = transfer_self(&cluster, sender).await;
    let tx3 = transfer_self(&cluster, sender).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2, &tx3]);

    let mut client = new_ledger_client(&cluster).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(
        !resp.checkpoints.is_empty(),
        "expected matching checkpoints"
    );
    let mut prev_hi: Option<u64> = None;
    for item in &resp.checkpoints {
        let seq = checkpoint_sequence(item);
        let wm = item.watermark.as_ref().expect("checkpoint item watermark");
        assert_eq!(
            wm.checkpoint,
            Some(seq),
            "ascending checkpoint item should claim its own sequence number complete"
        );
        if let Some(prev) = prev_hi {
            assert!(
                seq >= prev,
                "checkpoint boundary must be non-decreasing ascending"
            );
        }
        prev_hi = Some(seq);
    }

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_descending(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(
        !resp.checkpoints.is_empty(),
        "expected matching checkpoints"
    );
    let mut prev_lo: Option<u64> = None;
    for item in &resp.checkpoints {
        let seq = checkpoint_sequence(item);
        let wm = item.watermark.as_ref().expect("checkpoint item watermark");
        assert_eq!(
            wm.checkpoint,
            Some(seq),
            "descending checkpoint item should claim its own sequence number complete"
        );
        if let Some(prev) = prev_lo {
            assert!(
                seq <= prev,
                "checkpoint boundary must be non-increasing descending"
            );
        }
        prev_lo = Some(seq);
    }
}

// The contrast that makes the checkpoint assertion above meaningful: list_events
// scans WITHIN a checkpoint, so an event at checkpoint C does not prove C
// complete (more matches may sit at higher event_seqs). The covered boundary
// must therefore EXCLUDE C itself — `checkpoint` == C - 1 ascending, C + 1
// descending — i.e. the under-claim is correct here and a bug for checkpoints.
#[sim_test]
async fn test_list_events_item_watermark_boundary() {
    let cluster = new_cluster().await;
    let sender = cluster.get_address_0();
    let (pkg, _) = publish_package(&cluster, sender, emit_test_event_pkg_path()).await;
    let tx1 = call_emit_many(&cluster, sender, pkg, 3).await;
    let tx2 = call_emit_many(&cluster, sender, pkg, 2).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2]);
    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));

    let mut client = new_ledger_client(&cluster).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    assert!(!resp.events.is_empty(), "expected matching events");
    let mut prev_hi: Option<u64> = None;
    for item in &resp.events {
        let cp = event_checkpoint(item).expect("event item checkpoint");
        assert!(cp >= 1, "user events are never in the genesis checkpoint");
        let expected_hi = cp - 1;
        let wm = item.watermark.as_ref().expect("event item watermark");
        assert_eq!(
            wm.checkpoint,
            Some(expected_hi),
            "ascending event item must under-claim its own checkpoint (C - 1)"
        );
        if let Some(prev) = prev_hi {
            assert!(
                expected_hi >= prev,
                "event checkpoint boundary must be non-decreasing ascending"
            );
        }
        prev_hi = Some(expected_hi);
    }

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(event_type_and_position_mask());
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options_descending(100));
    let resp = list_events_result(&mut client, req).await;
    assert!(!resp.events.is_empty(), "expected matching events");
    let mut prev_lo: Option<u64> = None;
    for item in &resp.events {
        let cp = event_checkpoint(item).expect("event item checkpoint");
        let expected_lo = cp + 1;
        let wm = item.watermark.as_ref().expect("event item watermark");
        assert_eq!(
            wm.checkpoint,
            Some(expected_lo),
            "descending event item must under-claim its own checkpoint (C + 1)"
        );
        if let Some(prev) = prev_lo {
            assert!(
                expected_lo <= prev,
                "event checkpoint boundary must be non-increasing descending"
            );
        }
        prev_lo = Some(expected_lo);
    }
}

// Natural completion (the scan drains the whole range without hitting the item
// limit) folds the terminal `Watermark` onto the final `QueryEnd` frame,
// claiming the range's final checkpoint complete. An item-limited query stops
// early with `ItemLimit` and repeats the last item watermark on that end frame,
// so the resume cursor is still visible at stream termination. (The mid-stream
// sparse-gap scan watermark is not reachable in e2e: it needs the scan to cross
// ~16M empty tx_seqs to exhaust the per-chunk bucket budget.)
#[sim_test]
async fn test_list_checkpoints_terminal_watermark() {
    let cluster = new_cluster().await;
    let sender = cluster.get_address_0();
    let tx1 = transfer_self(&cluster, sender).await;
    let tx2 = transfer_self(&cluster, sender).await;
    let tx3 = transfer_self(&cluster, sender).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2, &tx3]);

    let mut client = new_ledger_client(&cluster).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert_eq!(resp.end_reason, Some(QueryEndReason::CheckpointBound));
    assert_eq!(
        resp.watermarks.len(),
        1,
        "natural completion carries exactly one terminal watermark on the end frame"
    );
    let terminal = &resp.watermarks[0];
    let last_item_hi = resp
        .checkpoints
        .last()
        .and_then(|item| item.watermark.as_ref())
        .and_then(|wm| wm.checkpoint)
        .expect("last item checkpoint boundary");
    assert!(
        terminal.checkpoint.is_some_and(|hi| hi >= last_item_hi),
        "terminal watermark must not regress the covered boundary"
    );

    // The terminal cursor is a safe resume point: resuming past it returns nothing.
    let cursor = terminal.cursor.clone().expect("terminal watermark cursor");
    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(100, cursor));
    let resumed = list_checkpoints_result(&mut client, req).await;
    assert!(
        resumed.checkpoints.is_empty(),
        "resuming past the terminal watermark should yield no more items"
    );

    // Item-limited query: the end frame repeats the last item's watermark.
    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(2));
    let limited = list_checkpoints_result(&mut client, req).await;
    assert_item_limit_end(limited.end, limited.end_reason);
    let last_item_watermark = limited
        .checkpoints
        .last()
        .and_then(|item| item.watermark.as_ref())
        .expect("last item watermark");
    assert_eq!(
        limited.watermarks.len(),
        1,
        "item-limited query repeats the last item watermark on the end frame"
    );
    let end_watermark = &limited.watermarks[0];
    assert_eq!(end_watermark.cursor, last_item_watermark.cursor);
    assert_eq!(end_watermark.checkpoint, last_item_watermark.checkpoint);
}

#[sim_test]
async fn test_list_transactions_multi_leaf_tiny_budget_resumes() {
    // A 2-leaf AND filter (`sender` + `move_call`) evaluated under a budget so
    // tiny it equals the literal count (every leaf's `take_first` reservation
    // exactly exhausts the request budget) must drain its full matching set
    // across pages with continuation cursors and a clean terminal reason, never
    // a cursorless `QueryEnd`. All seeded data lives in bucket 0, so `take_first`
    // covers it and the scan completes naturally — this exercises the
    // merge/reservation path under budget pressure, not a `SCAN_LIMIT` stop (a
    // real multi-leaf bucket `SCAN_LIMIT` is unreachable full-stack; the
    // evaluator-level unit tests cover that classification).
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .with_rpc_config(sui_config::RpcConfig {
            enable_indexing: Some(true),
            ledger_history: Some(sui_config::rpc_config::LedgerHistoryConfig {
                bitmap_bucket_scan_budget: Some(2),
                chunk_bucket_scan_budget: Some(2),
                max_bitmap_filter_literals: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        })
        .build()
        .await;
    let sender = cluster.get_address_0();
    let other = cluster.get_address_1();

    let (pkg, _) = publish_package(&cluster, sender, emit_test_event_pkg_path()).await;
    let move_call_path = format!(
        "{}::emit_test_event::emit_test_event",
        pkg.to_canonical_string(true)
    );

    // Matching set: sender's move calls into the published package.
    let mut expected = HashSet::new();
    for _ in 0..4 {
        let tx = call_move(&cluster, sender, pkg, "emit_test_event", "emit_test_event").await;
        expected.insert(tx_digest(&tx));
    }
    // Noise that must NOT match: sender's non-move-call tx, and another
    // sender's move call into the same package — each fails exactly one leaf.
    transfer_self(&cluster, sender).await;
    call_move(&cluster, other, pkg, "emit_test_event", "emit_test_event").await;

    let mut client = new_ledger_client(&cluster).await;
    let filter = tx_and(vec![tx_sender(sender), tx_move_call(&move_call_path)]);

    let mut paged: HashSet<String> = HashSet::new();
    let mut after: Option<Bytes> = None;
    let mut iterations = 0;
    let final_reason = loop {
        iterations += 1;
        assert!(iterations <= 64, "pagination loop did not terminate");

        let mut req = ListTransactionsRequest::default();
        req.read_mask = Some(FieldMask::from_paths(["digest"]));
        req.filter = Some(filter.clone());
        req.options = Some(query_options_maybe_after(2, after.clone()));
        let resp = list_transactions_result(&mut client, req).await;
        assert!(resp.end, "every page should carry an end frame");
        assert_transaction_cursors(&resp);
        for digest in transaction_digest_set(&resp) {
            assert!(paged.insert(digest), "pages must not overlap");
        }
        match resp.end_reason {
            Some(QueryEndReason::ItemLimit) => {
                after = Some(transaction_end_cursor(
                    &resp,
                    "item-limited page should carry a resume cursor",
                ));
            }
            reason @ (Some(QueryEndReason::CheckpointBound) | Some(QueryEndReason::LedgerTip)) => {
                break reason;
            }
            other => panic!(
                "multi-leaf scan under tiny budget must end with a non-error \
                 reason (item-limit continuation or clean terminal), got {other:?}"
            ),
        }
    };

    assert_eq!(
        paged, expected,
        "paged union under tiny multi-leaf budget must equal the full matching set"
    );
    assert!(
        matches!(
            final_reason,
            Some(QueryEndReason::CheckpointBound) | Some(QueryEndReason::LedgerTip)
        ),
        "drain must terminate on a natural-completion reason, got {final_reason:?}"
    );
}
