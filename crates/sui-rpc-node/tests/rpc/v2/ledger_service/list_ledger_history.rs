// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors
//! `sui-e2e-tests/tests/rpc/v2/ledger_service/list_ledger_history.rs`.
//! Drives transactions through Simulacrum, then asserts on the
//! v2alpha `ListTransactions` / `ListEvents` / `ListCheckpoints`
//! shape. Each helper transaction is wrapped in its own
//! checkpoint so the e2e test's per-tx `checkpoint_range` math
//! still lines up.

use std::collections::HashSet;
use std::path::PathBuf;

use bytes::Bytes;
use move_core_types::ident_str;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient as V2LedgerServiceClient;
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
use sui_rpc::proto::sui::rpc::v2alpha::Watermark;
use sui_rpc::proto::sui::rpc::v2alpha::event_literal;
use sui_rpc::proto::sui::rpc::v2alpha::event_predicate;
use sui_rpc::proto::sui::rpc::v2alpha::ledger_service_client::LedgerServiceClient as AlphaLedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2alpha::list_checkpoints_response;
use sui_rpc::proto::sui::rpc::v2alpha::list_events_response;
use sui_rpc::proto::sui::rpc::v2alpha::list_transactions_response;
use sui_rpc::proto::sui::rpc::v2alpha::transaction_literal;
use sui_rpc::proto::sui::rpc::v2alpha::transaction_predicate;
use sui_sdk_types::Digest;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use tokio::sync::Mutex;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;
const DEFAULT_CHECKPOINT_RANGE_END: u64 = 3_000_000;
const ACCOUNT_FUNDING: u64 = 100_000_000_000;

// ---------------------------------------------------------------
// Test cluster + sender bookkeeping.
// ---------------------------------------------------------------

/// A `LocalCluster` plus three funded senders, with their gas /
/// keypair tracked so successive transactions can flow without
/// re-querying the cluster. Indexed mutably via [`Self::sender`]
/// so per-transaction gas refs stay current.
struct Cluster {
    inner: LocalCluster,
    senders: Vec<Mutex<Sender>>,
    rgp: u64,
}

struct Sender {
    address: SuiAddress,
    keypair: AccountKeyPair,
    gas: ObjectRef,
}

impl Cluster {
    async fn new() -> Self {
        let inner = LocalCluster::new().await.unwrap();
        let rgp = inner.reference_gas_price().await;
        let mut senders = Vec::new();
        for _ in 0..3 {
            let (address, keypair, gas) = inner.funded_account(ACCOUNT_FUNDING).await.unwrap();
            senders.push(Mutex::new(Sender {
                address,
                keypair,
                gas,
            }));
        }
        inner.create_checkpoint().await.unwrap();
        Self {
            inner,
            senders,
            rgp,
        }
    }

    fn address(&self, idx: usize) -> SuiAddress {
        self.senders[idx]
            .try_lock()
            .ok()
            .map(|s| s.address)
            .unwrap_or_else(|| panic!("sender {idx} address contended"))
    }
}

async fn alpha_client(cluster: &Cluster) -> AlphaLedgerServiceClient<Channel> {
    AlphaLedgerServiceClient::connect(cluster.inner.grpc_url().to_string())
        .await
        .unwrap()
}

async fn latest_checkpoint_sequence(cluster: &Cluster) -> u64 {
    let mut client = V2LedgerServiceClient::connect(cluster.inner.grpc_url().to_string())
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
        .and_then(|c| c.sequence_number)
        .expect("latest checkpoint sequence")
}

/// Submit `tx_data` signed by the sender at `sender_idx`,
/// produce a checkpoint, look up the ExecutedTransaction over
/// gRPC, and refresh the sender's gas ref to the post-tx
/// version. Mirrors the e2e test's `execute_programmable`
/// helper that goes through `execute_transaction_and_wait_for_checkpoint`.
async fn execute(
    cluster: &Cluster,
    sender_idx: usize,
    tx_data: TransactionData,
) -> ExecutedTransaction {
    let mut sender = cluster.senders[sender_idx].lock().await;
    let signed = to_sender_signed_transaction(tx_data, &sender.keypair);
    let digest: Digest = (*signed.digest()).into();
    let (effects, err) = cluster.inner.execute_transaction(signed).await.unwrap();
    assert!(err.is_none(), "test transaction must succeed: {err:?}");
    sender.gas = effects.gas_object().expect("gas object always present").0;
    drop(sender);
    cluster.inner.create_checkpoint().await.unwrap();

    let mut client = V2LedgerServiceClient::connect(cluster.inner.grpc_url().to_string())
        .await
        .unwrap();
    client
        .get_transaction(
            GetTransactionRequest::new(&digest).with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap()
}

async fn build_tx_data(
    cluster: &Cluster,
    sender_idx: usize,
    builder: ProgrammableTransactionBuilder,
) -> TransactionData {
    let sender = cluster.senders[sender_idx].lock().await;
    TransactionData::new_programmable(
        sender.address,
        vec![sender.gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.rgp,
    )
}

async fn transfer_self(cluster: &Cluster, sender_idx: usize) -> ExecutedTransaction {
    let address = cluster.address(sender_idx);
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(address, None);
    let tx_data = build_tx_data(cluster, sender_idx, builder).await;
    execute(cluster, sender_idx, tx_data).await
}

async fn split_transfer(
    cluster: &Cluster,
    sender_idx: usize,
    recipient: SuiAddress,
) -> (ExecutedTransaction, ObjectID) {
    let gas_id = {
        let sender = cluster.senders[sender_idx].lock().await;
        sender.gas.0
    };
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(recipient, Some(1_000_000));
    let tx_data = build_tx_data(cluster, sender_idx, builder).await;
    (execute(cluster, sender_idx, tx_data).await, gas_id)
}

async fn publish_package(cluster: &Cluster, sender_idx: usize, path: PathBuf) -> ObjectID {
    let mut sender = cluster.senders[sender_idx].lock().await;
    let (package_id, effects) = cluster
        .inner
        .publish_package(sender.address, &sender.keypair, sender.gas, path)
        .await
        .unwrap();
    sender.gas = effects.gas_object().unwrap().0;
    drop(sender);
    cluster.inner.create_checkpoint().await.unwrap();
    package_id
}

async fn call_move(
    cluster: &Cluster,
    sender_idx: usize,
    pkg: ObjectID,
    module: &str,
    function: &str,
) -> ExecutedTransaction {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.programmable_move_call(
        pkg,
        move_core_types::identifier::Identifier::new(module).unwrap(),
        move_core_types::identifier::Identifier::new(function).unwrap(),
        vec![],
        vec![],
    );
    let tx_data = build_tx_data(cluster, sender_idx, builder).await;
    execute(cluster, sender_idx, tx_data).await
}

async fn call_emit_many(
    cluster: &Cluster,
    sender_idx: usize,
    pkg: ObjectID,
    count: u64,
) -> ExecutedTransaction {
    let mut builder = ProgrammableTransactionBuilder::new();
    let arg = builder.pure(count).unwrap();
    builder.programmable_move_call(
        pkg,
        ident_str!("emit_test_event").to_owned(),
        ident_str!("emit_many").to_owned(),
        vec![],
        vec![arg],
    );
    let tx_data = build_tx_data(cluster, sender_idx, builder).await;
    execute(cluster, sender_idx, tx_data).await
}

fn data_path(parts: &[&str]) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("..");
    p.push("sui-e2e-tests");
    p.push("tests");
    p.push("rpc");
    p.push("data");
    for part in parts {
        p.push(part);
    }
    p
}

fn emit_test_event_pkg_path() -> PathBuf {
    data_path(&["ledger_history", "event", "emit_test_event"])
}

fn authenticated_event_pkg_path() -> PathBuf {
    data_path(&["ledger_history", "event", "authenticated_event"])
}

fn generic_event_pkg_path() -> PathBuf {
    data_path(&["ledger_history", "event", "generic_event"])
}

// ---------------------------------------------------------------
// Response collectors mirroring the e2e helpers.
// ---------------------------------------------------------------

struct TransactionsResult {
    transactions: Vec<TransactionItem>,
    end: bool,
    end_cursor: Option<Bytes>,
    end_reason: Option<QueryEndReason>,
}

struct EventsResult {
    events: Vec<EventItem>,
    end: bool,
    end_cursor: Option<Bytes>,
    end_reason: Option<QueryEndReason>,
}

struct CheckpointsResult {
    checkpoints: Vec<CheckpointItem>,
    watermarks: Vec<Watermark>,
    end: bool,
    end_cursor: Option<Bytes>,
    end_reason: Option<QueryEndReason>,
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
    let mut end_cursor = None;
    let mut end_reason = None;
    while let Some(response) = stream.message().await.unwrap() {
        match response.response.expect("list_transactions response frame") {
            list_transactions_response::Response::Item(item) => {
                assert!(!end, "item frame after end");
                if let Some(c) = item.watermark.as_ref().and_then(|w| w.cursor.clone()) {
                    end_cursor = Some(c);
                }
                transactions.push(item);
            }
            list_transactions_response::Response::Watermark(wm) => {
                assert!(!end, "watermark frame after end");
                if let Some(c) = wm.cursor {
                    end_cursor = Some(c);
                }
            }
            list_transactions_response::Response::End(end_frame) => {
                assert!(!end, "duplicate end");
                end = true;
                end_reason = Some(QueryEndReason::try_from(end_frame.reason).unwrap());
            }
            other => panic!("unexpected list_transactions frame: {other:?}"),
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
    let mut end_cursor = None;
    let mut end_reason = None;
    while let Some(response) = stream.message().await.unwrap() {
        match response.response.expect("list_events response frame") {
            list_events_response::Response::Item(item) => {
                assert!(!end, "item frame after end");
                if let Some(c) = item.watermark.as_ref().and_then(|w| w.cursor.clone()) {
                    end_cursor = Some(c);
                }
                events.push(item);
            }
            list_events_response::Response::Watermark(wm) => {
                assert!(!end, "watermark frame after end");
                if let Some(c) = wm.cursor {
                    end_cursor = Some(c);
                }
            }
            list_events_response::Response::End(end_frame) => {
                assert!(!end, "duplicate end");
                end = true;
                end_reason = Some(QueryEndReason::try_from(end_frame.reason).unwrap());
            }
            other => panic!("unexpected list_events frame: {other:?}"),
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
    let mut end_cursor = None;
    let mut end_reason = None;
    while let Some(response) = stream.message().await.unwrap() {
        match response.response.expect("list_checkpoints response frame") {
            list_checkpoints_response::Response::Item(item) => {
                assert!(!end, "item frame after end");
                if let Some(c) = item.watermark.as_ref().and_then(|w| w.cursor.clone()) {
                    end_cursor = Some(c);
                }
                checkpoints.push(item);
            }
            list_checkpoints_response::Response::Watermark(wm) => {
                assert!(!end, "watermark frame after end");
                if let Some(c) = wm.cursor.clone() {
                    end_cursor = Some(c);
                }
                watermarks.push(wm);
            }
            list_checkpoints_response::Response::End(end_frame) => {
                assert!(!end, "duplicate end");
                end = true;
                end_reason = Some(QueryEndReason::try_from(end_frame.reason).unwrap());
            }
            other => panic!("unexpected list_checkpoints frame: {other:?}"),
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

async fn expect_invalid_tx(
    client: &mut AlphaLedgerServiceClient<Channel>,
    req: ListTransactionsRequest,
) {
    let err = client
        .list_transactions(req)
        .await
        .expect_err("InvalidArgument expected");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

async fn expect_invalid_ev(client: &mut AlphaLedgerServiceClient<Channel>, req: ListEventsRequest) {
    let err = client
        .list_events(req)
        .await
        .expect_err("InvalidArgument expected");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

async fn expect_invalid_cp(
    client: &mut AlphaLedgerServiceClient<Channel>,
    req: ListCheckpointsRequest,
) {
    let err = client
        .list_checkpoints(req)
        .await
        .expect_err("InvalidArgument expected");
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

// ---------------------------------------------------------------
// QueryOptions / filter builders (one-to-one with the e2e file).
// ---------------------------------------------------------------

fn query_options(limit_items: u32) -> QueryOptions {
    let mut o = QueryOptions::default();
    o.limit_items = Some(limit_items);
    o
}
fn query_options_after(limit_items: u32, after: Bytes) -> QueryOptions {
    let mut o = query_options(limit_items);
    o.after = Some(after);
    o
}
fn query_options_maybe_after(limit_items: u32, after: Option<Bytes>) -> QueryOptions {
    let mut o = query_options(limit_items);
    o.after = after;
    o
}
fn query_options_descending(limit_items: u32) -> QueryOptions {
    let mut o = query_options(limit_items);
    o.ordering = Ordering::Descending as i32;
    o
}
fn query_options_descending_before(limit_items: u32, before: Bytes) -> QueryOptions {
    let mut o = query_options_descending(limit_items);
    o.before = Some(before);
    o
}
fn query_options_descending_maybe_before(limit_items: u32, before: Option<Bytes>) -> QueryOptions {
    let mut o = query_options_descending(limit_items);
    o.before = before;
    o
}
fn query_options_between(limit_items: u32, after: Bytes, before: Bytes) -> QueryOptions {
    let mut o = query_options(limit_items);
    o.after = Some(after);
    o.before = Some(before);
    o
}
fn query_options_between_descending(limit_items: u32, after: Bytes, before: Bytes) -> QueryOptions {
    let mut o = query_options_between(limit_items, after, before);
    o.ordering = Ordering::Descending as i32;
    o
}

fn assert_item_limit_end(end: bool, reason: Option<QueryEndReason>) {
    assert!(end, "item-limit response should include end frame");
    assert_eq!(reason, Some(QueryEndReason::ItemLimit));
}

fn item_has_cursor(wm: Option<&Watermark>) -> bool {
    wm.is_some_and(|w| w.cursor.is_some())
}

fn assert_tx_cursors(r: &TransactionsResult) {
    for item in &r.transactions {
        assert!(
            item_has_cursor(item.watermark.as_ref()),
            "tx item should have cursor"
        );
    }
}
fn assert_ev_cursors(r: &EventsResult) {
    for item in &r.events {
        assert!(
            item_has_cursor(item.watermark.as_ref()),
            "ev item should have cursor"
        );
    }
}
fn assert_cp_cursors(r: &CheckpointsResult) {
    for item in &r.checkpoints {
        assert!(
            item_has_cursor(item.watermark.as_ref()),
            "cp item should have cursor"
        );
    }
}

fn checkpoint_sequence(item: &CheckpointItem) -> u64 {
    item.checkpoint
        .as_ref()
        .and_then(|c| c.sequence_number)
        .expect("cp item should have sequence_number")
}

fn tx_digest_str(tx: &ExecutedTransaction) -> String {
    tx.digest().to_owned()
}

fn tx_checkpoint(tx: &ExecutedTransaction) -> u64 {
    tx.checkpoint.expect("executed tx should carry checkpoint")
}

fn checkpoint_range(txs: &[&ExecutedTransaction]) -> (u64, u64) {
    let start = txs.iter().map(|tx| tx_checkpoint(tx)).min().unwrap();
    let end = txs.iter().map(|tx| tx_checkpoint(tx)).max().unwrap() + 1;
    (start, end)
}

fn transaction_digest_set(r: &TransactionsResult) -> HashSet<String> {
    r.transactions
        .iter()
        .filter_map(|t| t.transaction.as_ref().and_then(|tx| tx.digest.clone()))
        .collect()
}
fn event_digest_set(r: &EventsResult) -> HashSet<String> {
    r.events
        .iter()
        .filter_map(|e| e.transaction_digest.clone())
        .collect()
}

fn first_tx_cursor(r: &TransactionsResult, msg: &str) -> Bytes {
    r.transactions
        .first()
        .and_then(|i| i.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(msg)
}
fn last_tx_cursor(r: &TransactionsResult, msg: &str) -> Bytes {
    r.transactions
        .last()
        .and_then(|i| i.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(msg)
}
fn tx_end_cursor(r: &TransactionsResult, msg: &str) -> Bytes {
    r.end_cursor.clone().expect(msg)
}
fn first_ev_cursor(r: &EventsResult, msg: &str) -> Bytes {
    r.events
        .first()
        .and_then(|i| i.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(msg)
}
fn last_ev_cursor(r: &EventsResult, msg: &str) -> Bytes {
    r.events
        .last()
        .and_then(|i| i.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(msg)
}
fn ev_end_cursor(r: &EventsResult, msg: &str) -> Bytes {
    r.end_cursor.clone().expect(msg)
}
fn first_cp_cursor(r: &CheckpointsResult, msg: &str) -> Bytes {
    r.checkpoints
        .first()
        .and_then(|i| i.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(msg)
}
fn last_cp_cursor(r: &CheckpointsResult, msg: &str) -> Bytes {
    r.checkpoints
        .last()
        .and_then(|i| i.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .expect(msg)
}
fn cp_end_cursor(r: &CheckpointsResult, msg: &str) -> Bytes {
    r.end_cursor.clone().expect(msg)
}

// Filter builders.

fn tx_filter(literals: Vec<TransactionLiteral>) -> TransactionFilter {
    tx_filter_terms(vec![literals])
}
fn tx_filter_terms(terms: Vec<Vec<TransactionLiteral>>) -> TransactionFilter {
    let mut f = TransactionFilter::default();
    f.terms = terms
        .into_iter()
        .map(|literals| {
            let mut term = TransactionTerm::default();
            term.literals = literals;
            term
        })
        .collect();
    f
}
fn tx_or(terms: Vec<Vec<TransactionLiteral>>) -> TransactionFilter {
    tx_filter_terms(terms)
}
fn ev_filter(literals: Vec<EventLiteral>) -> EventFilter {
    ev_filter_terms(vec![literals])
}
fn ev_filter_terms(terms: Vec<Vec<EventLiteral>>) -> EventFilter {
    let mut f = EventFilter::default();
    f.terms = terms
        .into_iter()
        .map(|literals| {
            let mut term = EventTerm::default();
            term.literals = literals;
            term
        })
        .collect();
    f
}
fn ev_or(terms: Vec<Vec<EventLiteral>>) -> EventFilter {
    ev_filter_terms(terms)
}

fn tx_missing_include_filter(addr: SuiAddress) -> TransactionFilter {
    let mut term = TransactionTerm::default();
    term.literals = vec![tx_not_sender_literal(addr)];
    let mut f = TransactionFilter::default();
    f.terms = vec![term];
    f
}
fn ev_missing_include_filter(addr: SuiAddress) -> EventFilter {
    let mut term = EventTerm::default();
    term.literals = vec![ev_not_sender_literal(addr)];
    let mut f = EventFilter::default();
    f.terms = vec![term];
    f
}

fn tx_include(p: transaction_predicate::Predicate) -> TransactionLiteral {
    let mut pred = TransactionPredicate::default();
    pred.predicate = Some(p);
    let mut lit = TransactionLiteral::default();
    lit.polarity = Some(transaction_literal::Polarity::Include(pred));
    lit
}
fn tx_exclude(p: transaction_predicate::Predicate) -> TransactionLiteral {
    let mut pred = TransactionPredicate::default();
    pred.predicate = Some(p);
    let mut lit = TransactionLiteral::default();
    lit.polarity = Some(transaction_literal::Polarity::Exclude(pred));
    lit
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
fn tx_event_stream_head(id: ObjectID) -> TransactionFilter {
    tx_filter(vec![tx_event_stream_head_literal(id)])
}
fn tx_and(filters: Vec<TransactionFilter>) -> TransactionFilter {
    let mut literals = Vec::new();
    for f in filters {
        for term in f.terms {
            literals.extend(term.literals);
        }
    }
    tx_filter(literals)
}

fn ev_include(p: event_predicate::Predicate) -> EventLiteral {
    let mut pred = EventPredicate::default();
    pred.predicate = Some(p);
    let mut lit = EventLiteral::default();
    lit.polarity = Some(event_literal::Polarity::Include(pred));
    lit
}
fn ev_exclude(p: event_predicate::Predicate) -> EventLiteral {
    let mut pred = EventPredicate::default();
    pred.predicate = Some(p);
    let mut lit = EventLiteral::default();
    lit.polarity = Some(event_literal::Polarity::Exclude(pred));
    lit
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
fn ev_event_stream_head(id: ObjectID) -> EventFilter {
    ev_filter(vec![ev_event_stream_head_literal(id)])
}
fn ev_and(filters: Vec<EventFilter>) -> EventFilter {
    let mut literals = Vec::new();
    for f in filters {
        for term in f.terms {
            literals.extend(term.literals);
        }
    }
    ev_filter(literals)
}

// ---------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------

#[tokio::test]
async fn test_list_transactions_unfiltered_and_sender_filter() {
    let cluster = Cluster::new().await;
    let sender = cluster.address(0);
    let tx = transfer_self(&cluster, 0).await;
    let digest = tx_digest_str(&tx);

    let mut client = alpha_client(&cluster).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest", "checkpoint"]));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    assert_tx_cursors(&resp);
    assert!(
        resp.transactions.len() >= 2,
        "expected genesis plus transfer transactions"
    );
    assert!(transaction_digest_set(&resp).contains(&digest));
    for result in &resp.transactions {
        assert!(
            result
                .transaction
                .as_ref()
                .and_then(|tx| tx.checkpoint)
                .is_some(),
            "checkpoint should be present when requested",
        );
    }

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    assert_tx_cursors(&resp);
    assert!(transaction_digest_set(&resp).contains(&digest));
}

#[tokio::test]
async fn test_list_transactions_query_options() {
    let cluster = Cluster::new().await;
    let sender = cluster.address(0);
    let tx1 = transfer_self(&cluster, 0).await;
    let tx2 = transfer_self(&cluster, 0).await;
    let tx3 = transfer_self(&cluster, 0).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2, &tx3]);

    let mut client = alpha_client(&cluster).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(2));
    let r1 = list_transactions_result(&mut client, req).await;
    assert_eq!(r1.transactions.len(), 2);
    assert_item_limit_end(r1.end, r1.end_reason);
    assert_tx_cursors(&r1);
    let cursor = tx_end_cursor(&r1, "first response cursor");

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(2, cursor));
    let r2 = list_transactions_result(&mut client, req).await;
    assert_eq!(r2.transactions.len(), 1);
    assert!(r2.end);
    assert_eq!(r2.end_reason, Some(QueryEndReason::CheckpointBound));
    let final_cursor = last_tx_cursor(&r2, "final cursor");

    let first_digests = transaction_digest_set(&r1);
    for d in transaction_digest_set(&r2) {
        assert!(!first_digests.contains(&d));
    }

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(2, final_cursor));
    let r3 = list_transactions_result(&mut client, req).await;
    assert!(r3.transactions.is_empty());
    assert!(r3.end);

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_descending(2));
    let rev1 = list_transactions_result(&mut client, req).await;
    assert_eq!(rev1.transactions.len(), 2);
    assert_item_limit_end(rev1.end, rev1.end_reason);
    let cursor = tx_end_cursor(&rev1, "reverse cursor");

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_descending_before(2, cursor));
    let rev2 = list_transactions_result(&mut client, req).await;
    assert_eq!(rev2.transactions.len(), 1);
    assert!(rev2.end);

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(3));
    let exact = list_transactions_result(&mut client, req).await;
    assert_eq!(exact.transactions.len(), 3);
    let first_cursor = first_tx_cursor(&exact, "exact first cursor");
    let last_cursor = last_tx_cursor(&exact, "exact last cursor");

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
}

#[tokio::test]
async fn test_list_transactions_filter_predicates() {
    let cluster = Cluster::new().await;
    let sender_a = cluster.address(0);
    let sender_b = cluster.address(1);

    let pkg = publish_package(&cluster, 0, generic_event_pkg_path()).await;
    let tx_a = call_move(&cluster, 0, pkg, "generic_event", "emit_u64").await;
    let tx_b = call_move(&cluster, 1, pkg, "generic_event", "emit_address").await;
    let tx_c = transfer_self(&cluster, 2).await;
    let digest_a = tx_digest_str(&tx_a);
    let digest_b = tx_digest_str(&tx_b);
    let digest_c = tx_digest_str(&tx_c);

    let mut client = alpha_client(&cluster).await;
    let fetch = |client: &mut AlphaLedgerServiceClient<Channel>, filter: TransactionFilter| {
        let mut client = client.clone();
        async move {
            let mut req = ListTransactionsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["digest"]));
            req.filter = Some(filter);
            req.options = Some(query_options(100));
            list_transactions_result(&mut client, req).await
        }
    };

    let resp = fetch(
        &mut client,
        tx_or(vec![
            vec![tx_sender_literal(sender_a)],
            vec![tx_sender_literal(sender_b)],
        ]),
    )
    .await;
    let digests = transaction_digest_set(&resp);
    assert!(digests.contains(&digest_a) && digests.contains(&digest_b));
    assert!(!digests.contains(&digest_c));

    let pkg_path = pkg.to_canonical_string(true);
    let module_path = format!("{pkg_path}::generic_event");
    let emit_u64_path = format!("{module_path}::emit_u64");

    for filter in [tx_move_call(&pkg_path), tx_move_call(&module_path)] {
        let digests = transaction_digest_set(&fetch(&mut client, filter).await);
        assert!(digests.contains(&digest_a) && digests.contains(&digest_b));
        assert!(!digests.contains(&digest_c));
    }

    let digests = transaction_digest_set(&fetch(&mut client, tx_move_call(&emit_u64_path)).await);
    assert!(digests.contains(&digest_a) && !digests.contains(&digest_b));

    for filter in [tx_emit_module(&pkg_path), tx_emit_module(&module_path)] {
        let digests = transaction_digest_set(&fetch(&mut client, filter).await);
        assert!(digests.contains(&digest_a) && digests.contains(&digest_b));
        assert!(!digests.contains(&digest_c));
    }

    let u64_type = format!("{module_path}::GenericEvent<u64>");
    let digests = transaction_digest_set(&fetch(&mut client, tx_event_type(&u64_type)).await);
    assert!(digests.contains(&digest_a) && !digests.contains(&digest_b));
}

#[tokio::test]
async fn test_list_transactions_combinators_and_affected_filters() {
    let cluster = Cluster::new().await;
    let sender_a = cluster.address(0);
    let sender_b = cluster.address(1);

    let pkg = publish_package(&cluster, 0, emit_test_event_pkg_path()).await;
    let tx_a_call = call_move(&cluster, 0, pkg, "emit_test_event", "emit_test_event").await;
    let tx_a_transfer = transfer_self(&cluster, 0).await;
    let tx_b_call = call_move(&cluster, 1, pkg, "emit_test_event", "emit_test_event").await;
    let digest_a_call = tx_digest_str(&tx_a_call);
    let digest_a_transfer = tx_digest_str(&tx_a_transfer);
    let digest_b_call = tx_digest_str(&tx_b_call);

    let mut client = alpha_client(&cluster).await;
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
    assert!(digests.contains(&digest_a_call));
    assert!(!digests.contains(&digest_a_transfer));
    assert!(!digests.contains(&digest_b_call));

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

    let (transfer_to_b, affected_gas_id) = split_transfer(&cluster, 0, sender_b).await;
    let transfer_to_b_digest = tx_digest_str(&transfer_to_b);

    let mut affected_address = AffectedAddressFilter::default();
    affected_address.address = Some(sender_b.to_string());
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_filter(vec![tx_include(
        transaction_predicate::Predicate::AffectedAddress(affected_address),
    )]));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    assert!(transaction_digest_set(&resp).contains(&transfer_to_b_digest));

    let mut affected_object = AffectedObjectFilter::default();
    affected_object.object_id = Some(affected_gas_id.to_canonical_string(true));
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_filter(vec![tx_include(
        transaction_predicate::Predicate::AffectedObject(affected_object),
    )]));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut client, req).await;
    assert!(transaction_digest_set(&resp).contains(&transfer_to_b_digest));
}

#[tokio::test]
async fn test_list_events_unfiltered_and_emit_module_filter() {
    let cluster = Cluster::new().await;
    let pkg = publish_package(&cluster, 0, emit_test_event_pkg_path()).await;
    let event_tx = call_move(&cluster, 0, pkg, "emit_test_event", "emit_test_event").await;
    let event_digest = tx_digest_str(&event_tx);

    let mut client = alpha_client(&cluster).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    assert_ev_cursors(&resp);
    assert!(event_digest_set(&resp).contains(&event_digest));
    let event_type = resp
        .events
        .iter()
        .find(|e| e.transaction_digest.as_deref() == Some(event_digest.as_str()))
        .and_then(|e| e.event.as_ref())
        .and_then(|e| e.event_type.as_deref())
        .expect("emitted event_type present");
    assert!(event_type.contains("emit_test_event::TestEvent"));

    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    assert_ev_cursors(&resp);
    assert!(event_digest_set(&resp).contains(&event_digest));
    for event in &resp.events {
        let event_type = event
            .event
            .as_ref()
            .and_then(|e| e.event_type.as_deref())
            .expect("event_type present");
        assert!(event_type.contains("emit_test_event"));
    }
}

#[tokio::test]
async fn test_list_events_query_options_multi_event_tx() {
    let cluster = Cluster::new().await;
    let pkg = publish_package(&cluster, 0, emit_test_event_pkg_path()).await;
    let tx1 = call_emit_many(&cluster, 0, pkg, 5).await;
    let tx2 = call_emit_many(&cluster, 0, pkg, 3).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2]);
    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));

    let mut client = alpha_client(&cluster).await;
    let module_for_resp = module.clone();
    let response = |client: &mut AlphaLedgerServiceClient<Channel>, cursor: Option<Bytes>| {
        let mut client = client.clone();
        let module = module_for_resp.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["event_type"]));
            req.start_checkpoint = Some(start);
            req.end_checkpoint = Some(end);
            req.filter = Some(ev_emit_module(&module));
            req.options = Some(query_options_maybe_after(3, cursor));
            list_events_result(&mut client, req).await
        }
    };

    let r1 = response(&mut client, None).await;
    assert_eq!(r1.events.len(), 3);
    assert_item_limit_end(r1.end, r1.end_reason);
    assert_ev_cursors(&r1);
    let r1_cursor = ev_end_cursor(&r1, "response 1 cursor");

    let r2 = response(&mut client, Some(r1_cursor)).await;
    assert_eq!(r2.events.len(), 3);
    assert_item_limit_end(r2.end, r2.end_reason);
    assert_ev_cursors(&r2);
    let r2_cursor = ev_end_cursor(&r2, "response 2 cursor");

    let r3 = response(&mut client, Some(r2_cursor)).await;
    assert_eq!(r3.events.len(), 2);
    assert!(r3.end);
    assert_ev_cursors(&r3);

    let mut all_cursors: Vec<_> = r1
        .events
        .iter()
        .chain(r2.events.iter())
        .chain(r3.events.iter())
        .map(|e| e.watermark.as_ref().and_then(|w| w.cursor.clone()))
        .collect();
    let total = all_cursors.len();
    all_cursors.sort();
    all_cursors.dedup();
    assert_eq!(all_cursors.len(), total);
    assert_eq!(total, 8);

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options(8));
    let exact = list_events_result(&mut client, req).await;
    assert_eq!(exact.events.len(), 8);
    let first_cursor = first_ev_cursor(&exact, "exact first cursor");
    let last_cursor = last_ev_cursor(&exact, "exact last cursor");

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options_between(8, first_cursor, last_cursor.clone()));
    let bounded = list_events_result(&mut client, req).await;
    assert_eq!(bounded.events.len(), 6);
    assert_eq!(bounded.end_reason, Some(QueryEndReason::CursorBound));

    let module_for_reverse = module.clone();
    let reverse = |client: &mut AlphaLedgerServiceClient<Channel>, cursor: Option<Bytes>| {
        let mut client = client.clone();
        let module = module_for_reverse.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["event_type"]));
            req.start_checkpoint = Some(start);
            req.end_checkpoint = Some(end);
            req.filter = Some(ev_emit_module(&module));
            req.options = Some(query_options_descending_maybe_before(3, cursor));
            list_events_result(&mut client, req).await
        }
    };

    let rp1 = reverse(&mut client, None).await;
    assert_eq!(rp1.events.len(), 3);
    assert_item_limit_end(rp1.end, rp1.end_reason);
    let rp1_cursor = ev_end_cursor(&rp1, "reverse cursor 1");
    let rp2 = reverse(&mut client, Some(rp1_cursor)).await;
    assert_eq!(rp2.events.len(), 3);
    let rp2_cursor = ev_end_cursor(&rp2, "reverse cursor 2");
    let rp3 = reverse(&mut client, Some(rp2_cursor)).await;
    assert_eq!(rp3.events.len(), 2);
    assert!(rp3.end);

    let mut reverse_keys: Vec<_> = rp1
        .events
        .iter()
        .chain(rp2.events.iter())
        .chain(rp3.events.iter())
        .map(|e| (e.transaction_digest.clone(), e.event_index))
        .collect();
    let len = reverse_keys.len();
    reverse_keys.sort_unstable();
    reverse_keys.dedup();
    assert_eq!(reverse_keys.len(), len);

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options_after(8, last_cursor));
    let after_exact = list_events_result(&mut client, req).await;
    assert!(after_exact.events.is_empty());
    assert!(after_exact.end);
}

#[tokio::test]
async fn test_list_events_filter_predicates() {
    let cluster = Cluster::new().await;
    let sender_a = cluster.address(0);
    let sender_b = cluster.address(1);

    let generic_pkg = publish_package(&cluster, 0, generic_event_pkg_path()).await;
    let tx_u64 = call_move(&cluster, 0, generic_pkg, "generic_event", "emit_u64").await;
    let tx_addr_b = call_move(&cluster, 1, generic_pkg, "generic_event", "emit_address").await;
    let tx_addr_c = call_move(&cluster, 2, generic_pkg, "generic_event", "emit_address").await;
    let digest_u64 = tx_digest_str(&tx_u64);
    let digest_addr_b = tx_digest_str(&tx_addr_b);
    let digest_addr_c = tx_digest_str(&tx_addr_c);

    let mut client = alpha_client(&cluster).await;
    let fetch = |client: &mut AlphaLedgerServiceClient<Channel>, filter: EventFilter| {
        let mut client = client.clone();
        async move {
            let mut req = ListEventsRequest::default();
            req.read_mask = Some(FieldMask::from_paths(["event_type"]));
            req.filter = Some(filter);
            req.options = Some(query_options(100));
            list_events_result(&mut client, req).await
        }
    };

    let resp = fetch(
        &mut client,
        ev_or(vec![
            vec![ev_sender_literal(sender_a)],
            vec![ev_sender_literal(sender_b)],
        ]),
    )
    .await;
    let digests = event_digest_set(&resp);
    assert!(digests.contains(&digest_u64) && digests.contains(&digest_addr_b));
    assert!(!digests.contains(&digest_addr_c));

    let pkg_hex = generic_pkg.to_canonical_string(true);
    let module = format!("{pkg_hex}::generic_event");
    let name = format!("{module}::GenericEvent");
    let resp = fetch(&mut client, ev_event_type(&name)).await;
    let digests = event_digest_set(&resp);
    assert!(
        digests.contains(&digest_u64)
            && digests.contains(&digest_addr_b)
            && digests.contains(&digest_addr_c),
    );

    let u64_type = format!("{module}::GenericEvent<u64>");
    let resp = fetch(&mut client, ev_event_type(&u64_type)).await;
    let digests = event_digest_set(&resp);
    assert!(digests.contains(&digest_u64) && !digests.contains(&digest_addr_b));

    let resp = fetch(&mut client, ev_event_type(&module)).await;
    let digests = event_digest_set(&resp);
    assert!(digests.contains(&digest_u64) && digests.contains(&digest_addr_b));

    let auth_pkg = publish_package(&cluster, 0, authenticated_event_pkg_path()).await;
    let auth_tx = call_move(&cluster, 0, auth_pkg, "authenticated_event", "emit_both").await;
    let auth_digest = tx_digest_str(&auth_tx);

    let resp = fetch(&mut client, ev_event_stream_head(auth_pkg)).await;
    assert_eq!(resp.events.len(), 1);
    assert_eq!(
        resp.events[0].transaction_digest.as_deref(),
        Some(auth_digest.as_str()),
    );
    let event_type = resp.events[0]
        .event
        .as_ref()
        .and_then(|e| e.event_type.as_deref())
        .expect("authenticated event_type");
    assert!(event_type.contains("authenticated_event::AuthenticatedEvent"));

    let mut tx_client = client.clone();
    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.filter = Some(tx_event_stream_head(auth_pkg));
    req.options = Some(query_options(100));
    let resp = list_transactions_result(&mut tx_client, req).await;
    assert!(transaction_digest_set(&resp).contains(&auth_digest));
}

#[tokio::test]
async fn test_list_events_combinators() {
    let cluster = Cluster::new().await;
    let sender_a = cluster.address(0);
    let sender_b = cluster.address(1);

    let pkg = publish_package(&cluster, 0, emit_test_event_pkg_path()).await;
    let tx_a = call_move(&cluster, 0, pkg, "emit_test_event", "emit_test_event").await;
    let tx_b = call_move(&cluster, 1, pkg, "emit_test_event", "emit_test_event").await;
    let digest_a = tx_digest_str(&tx_a);
    let digest_b = tx_digest_str(&tx_b);

    let mut client = alpha_client(&cluster).await;
    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(ev_and(vec![ev_sender(sender_a), ev_emit_module(&module)]));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    let digests = event_digest_set(&resp);
    assert!(digests.contains(&digest_a));
    assert!(!digests.contains(&digest_b));

    let _ = ev_exclude; // helper exercised below
    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(ev_filter(vec![
        ev_sender_literal(sender_a),
        ev_not_sender_literal(sender_b),
    ]));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    let digests = event_digest_set(&resp);
    assert!(digests.contains(&digest_a));
    assert!(!digests.contains(&digest_b));
}

#[tokio::test]
async fn test_list_filter_edge_cases_and_limit_caps() {
    let cluster = Cluster::new().await;
    let sender = cluster.address(0);
    transfer_self(&cluster, 0).await;

    let mut client = alpha_client(&cluster).await;
    let beyond_tip = latest_checkpoint_sequence(&cluster).await + DEFAULT_CHECKPOINT_RANGE_END;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(beyond_tip);
    req.options = Some(query_options(10));
    let resp = list_transactions_result(&mut client, req).await;
    assert!(resp.transactions.is_empty());
    assert!(resp.end);
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(beyond_tip);
    req.options = Some(query_options(10));
    let resp = list_events_result(&mut client, req).await;
    assert!(resp.events.is_empty());
    assert!(resp.end);
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(beyond_tip);
    req.options = Some(query_options(10));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(resp.checkpoints.is_empty());
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
    assert!(resp.transactions.is_empty());
    assert!(resp.end);

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.filter = Some(ev_sender(never_sender));
    req.options = Some(query_options(10));
    let resp = list_events_result(&mut client, req).await;
    assert!(resp.events.is_empty());
    assert!(resp.end);

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(tx_sender(never_sender));
    req.options = Some(query_options(10));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(resp.checkpoints.is_empty());
    assert!(resp.end);

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(tx_move_call("0x1::a::b::c"));
    req.options = Some(query_options(10));
    expect_invalid_tx(&mut client, req).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(ev_event_type("0x1<u64>"));
    req.options = Some(query_options(10));
    expect_invalid_ev(&mut client, req).await;

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
    expect_invalid_tx(&mut client, req).await;

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
    expect_invalid_tx(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    let mut bad_options = query_options(10);
    bad_options.ordering = 99;
    req.options = Some(bad_options);
    expect_invalid_tx(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(TransactionFilter::default());
    req.options = Some(query_options(10));
    expect_invalid_tx(&mut client, req).await;

    let mut req = ListTransactionsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["digest"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(tx_missing_include_filter(sender));
    req.options = Some(query_options(10));
    expect_invalid_tx(&mut client, req).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(EventFilter::default());
    req.options = Some(query_options(10));
    expect_invalid_ev(&mut client, req).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(ev_missing_include_filter(sender));
    req.options = Some(query_options(10));
    expect_invalid_ev(&mut client, req).await;

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
    expect_invalid_cp(&mut client, req).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(TransactionFilter::default());
    req.options = Some(query_options(10));
    expect_invalid_cp(&mut client, req).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(0);
    req.end_checkpoint = Some(DEFAULT_CHECKPOINT_RANGE_END);
    req.filter = Some(tx_missing_include_filter(sender));
    req.options = Some(query_options(10));
    expect_invalid_cp(&mut client, req).await;

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

#[tokio::test]
async fn test_list_checkpoints_filters_and_ordering() {
    let cluster = Cluster::new().await;
    let sender_a = cluster.address(0);
    let sender_b = cluster.address(1);

    let tx_a = transfer_self(&cluster, 0).await;
    let tx_b = transfer_self(&cluster, 1).await;
    let tx_c = transfer_self(&cluster, 2).await;
    let cp_a = tx_checkpoint(&tx_a);
    let cp_b = tx_checkpoint(&tx_b);
    let cp_c = tx_checkpoint(&tx_c);

    let mut client = alpha_client(&cluster).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert_cp_cursors(&resp);
    assert!(resp.checkpoints.len() >= 4);
    for window in resp.checkpoints.windows(2) {
        let a = checkpoint_sequence(&window[0]);
        let b = checkpoint_sequence(&window[1]);
        assert!(a < b);
    }

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(tx_sender(sender_a));
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    let seqs: HashSet<u64> = resp.checkpoints.iter().map(checkpoint_sequence).collect();
    assert!(seqs.contains(&cp_a));

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.filter = Some(tx_or(vec![
        vec![tx_sender_literal(sender_a)],
        vec![tx_sender_literal(sender_b)],
    ]));
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    let seqs: HashSet<u64> = resp.checkpoints.iter().map(checkpoint_sequence).collect();
    assert!(seqs.contains(&cp_a) && seqs.contains(&cp_b));
    assert!(!seqs.contains(&cp_c));

    let pkg = publish_package(&cluster, 0, emit_test_event_pkg_path()).await;
    let tx_a_call = call_move(&cluster, 0, pkg, "emit_test_event", "emit_test_event").await;
    let tx_a_transfer = transfer_self(&cluster, 0).await;
    let tx_b_call = call_move(&cluster, 1, pkg, "emit_test_event", "emit_test_event").await;
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
    let seqs: HashSet<u64> = resp.checkpoints.iter().map(checkpoint_sequence).collect();
    assert!(seqs.contains(&cp_a_call));
    assert!(!seqs.contains(&cp_a_transfer));
    assert!(!seqs.contains(&cp_b_call));
}

#[tokio::test]
async fn test_list_checkpoints_query_options() {
    let cluster = Cluster::new().await;
    let sender = cluster.address(0);
    let tx1 = transfer_self(&cluster, 0).await;
    let tx2 = transfer_self(&cluster, 0).await;
    let tx3 = transfer_self(&cluster, 0).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2, &tx3]);

    let mut client = alpha_client(&cluster).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(2));
    let r1 = list_checkpoints_result(&mut client, req).await;
    assert_eq!(r1.checkpoints.len(), 2);
    assert_item_limit_end(r1.end, r1.end_reason);
    let cursor = cp_end_cursor(&r1, "first cursor");
    let r1_seqs: Vec<_> = r1.checkpoints.iter().map(checkpoint_sequence).collect();

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(2, cursor));
    let r2 = list_checkpoints_result(&mut client, req).await;
    assert_eq!(r2.checkpoints.len(), 1);
    assert!(r2.end);
    assert_eq!(r2.end_reason, Some(QueryEndReason::CheckpointBound));
    let r2_seqs: Vec<_> = r2.checkpoints.iter().map(checkpoint_sequence).collect();
    for seq in &r2_seqs {
        assert!(!r1_seqs.contains(seq));
        assert!(*seq > *r1_seqs.last().unwrap());
    }

    let final_cursor = last_cp_cursor(&r2, "final cursor");
    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(2, final_cursor));
    let r3 = list_checkpoints_result(&mut client, req).await;
    assert!(r3.checkpoints.is_empty());
    assert!(r3.end);
    assert_eq!(r3.end_reason, Some(QueryEndReason::CursorBound));

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_descending(2));
    let rev1 = list_checkpoints_result(&mut client, req).await;
    assert_eq!(rev1.checkpoints.len(), 2);
    assert_item_limit_end(rev1.end, rev1.end_reason);
    let rev1_seqs: Vec<_> = rev1.checkpoints.iter().map(checkpoint_sequence).collect();
    assert!(rev1_seqs.windows(2).all(|p| p[0] > p[1]));
    let cursor = cp_end_cursor(&rev1, "reverse cursor");

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_descending_before(2, cursor));
    let rev2 = list_checkpoints_result(&mut client, req).await;
    assert_eq!(rev2.checkpoints.len(), 1);
    assert!(rev2.end);
    let rev2_seqs: Vec<_> = rev2.checkpoints.iter().map(checkpoint_sequence).collect();
    assert!(rev2_seqs.iter().all(|s| *s < *rev1_seqs.last().unwrap()));

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(3));
    let exact = list_checkpoints_result(&mut client, req).await;
    assert_eq!(exact.checkpoints.len(), 3);
    let first_cursor = first_cp_cursor(&exact, "exact first");
    let last_cursor = last_cp_cursor(&exact, "exact last");

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

#[tokio::test]
async fn test_list_checkpoints_read_masks_and_empty_range() {
    let cluster = Cluster::new().await;

    let mut client = alpha_client(&cluster).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint =
        Some(latest_checkpoint_sequence(&cluster).await + DEFAULT_CHECKPOINT_RANGE_END);
    req.options = Some(query_options(10));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(resp.checkpoints.is_empty());
    assert!(resp.end);
    assert_eq!(resp.end_reason, Some(QueryEndReason::LedgerTip));

    let tx1 = transfer_self(&cluster, 0).await;
    let tx2 = transfer_self(&cluster, 0).await;
    let expected_digests = [tx_digest_str(&tx1), tx_digest_str(&tx2)];
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
    let returned: HashSet<String> = resp
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
    for d in &expected_digests {
        assert!(returned.contains(d));
    }

    let tx = transfer_self(&cluster, 0).await;
    let cp = tx_checkpoint(&tx);
    // The gas object id for the latest transaction.
    let gas_id = {
        let sender = cluster.senders[0].lock().await;
        sender.gas.0.to_canonical_string(true)
    };

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

    let saw_gas = resp.checkpoints.iter().any(|item| {
        item.checkpoint
            .as_ref()
            .and_then(|cp| cp.objects.as_ref())
            .is_some_and(|objs| {
                objs.objects
                    .iter()
                    .any(|o| o.object_id.as_deref() == Some(gas_id.as_str()))
            })
    });
    assert!(
        saw_gas,
        "expected gas object {gas_id} in checkpoint objects[]"
    );

    let any_tx = resp.checkpoints.iter().any(|item| {
        item.checkpoint
            .as_ref()
            .is_some_and(|c| c.transactions.iter().any(|tx| tx.digest.is_some()))
    });
    assert!(any_tx);
}

#[tokio::test]
async fn test_list_checkpoints_item_watermark_boundary() {
    let cluster = Cluster::new().await;
    let sender = cluster.address(0);
    let tx1 = transfer_self(&cluster, 0).await;
    let tx2 = transfer_self(&cluster, 0).await;
    let tx3 = transfer_self(&cluster, 0).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2, &tx3]);

    let mut client = alpha_client(&cluster).await;

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(100));
    let resp = list_checkpoints_result(&mut client, req).await;
    assert!(!resp.checkpoints.is_empty());
    let mut prev_hi: Option<u64> = None;
    for item in &resp.checkpoints {
        let seq = checkpoint_sequence(item);
        let wm = item.watermark.as_ref().expect("cp watermark");
        assert_eq!(wm.checkpoint_hi, Some(seq));
        assert_eq!(wm.checkpoint_lo, None);
        if let Some(prev) = prev_hi {
            assert!(seq >= prev);
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
    assert!(!resp.checkpoints.is_empty());
    let mut prev_lo: Option<u64> = None;
    for item in &resp.checkpoints {
        let seq = checkpoint_sequence(item);
        let wm = item.watermark.as_ref().expect("cp watermark");
        assert_eq!(wm.checkpoint_lo, Some(seq));
        assert_eq!(wm.checkpoint_hi, None);
        if let Some(prev) = prev_lo {
            assert!(seq <= prev);
        }
        prev_lo = Some(seq);
    }
}

#[tokio::test]
async fn test_list_events_item_watermark_boundary() {
    let cluster = Cluster::new().await;
    let pkg = publish_package(&cluster, 0, emit_test_event_pkg_path()).await;
    let tx1 = call_emit_many(&cluster, 0, pkg, 3).await;
    let tx2 = call_emit_many(&cluster, 0, pkg, 2).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2]);
    let module = format!("{}::emit_test_event", pkg.to_canonical_string(true));

    let mut client = alpha_client(&cluster).await;

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options(100));
    let resp = list_events_result(&mut client, req).await;
    assert!(!resp.events.is_empty());
    let mut prev_hi: Option<u64> = None;
    for item in &resp.events {
        let cp = item.checkpoint.expect("event item checkpoint");
        assert!(cp >= 1);
        let expected_hi = cp - 1;
        let wm = item.watermark.as_ref().expect("event watermark");
        assert_eq!(wm.checkpoint_hi, Some(expected_hi));
        assert_eq!(wm.checkpoint_lo, None);
        if let Some(prev) = prev_hi {
            assert!(expected_hi >= prev);
        }
        prev_hi = Some(expected_hi);
    }

    let mut req = ListEventsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["event_type"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(ev_emit_module(&module));
    req.options = Some(query_options_descending(100));
    let resp = list_events_result(&mut client, req).await;
    assert!(!resp.events.is_empty());
    let mut prev_lo: Option<u64> = None;
    for item in &resp.events {
        let cp = item.checkpoint.expect("event item checkpoint");
        let expected_lo = cp + 1;
        let wm = item.watermark.as_ref().expect("event watermark");
        assert_eq!(wm.checkpoint_lo, Some(expected_lo));
        assert_eq!(wm.checkpoint_hi, None);
        if let Some(prev) = prev_lo {
            assert!(expected_lo <= prev);
        }
        prev_lo = Some(expected_lo);
    }
}

#[tokio::test]
async fn test_list_checkpoints_terminal_watermark() {
    let cluster = Cluster::new().await;
    let sender = cluster.address(0);
    let tx1 = transfer_self(&cluster, 0).await;
    let tx2 = transfer_self(&cluster, 0).await;
    let tx3 = transfer_self(&cluster, 0).await;
    let (start, end) = checkpoint_range(&[&tx1, &tx2, &tx3]);

    let mut client = alpha_client(&cluster).await;

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
        "natural completion emits exactly one terminal watermark frame",
    );
    let terminal = &resp.watermarks[0];
    let last_item_hi = resp
        .checkpoints
        .last()
        .and_then(|item| item.watermark.as_ref())
        .and_then(|wm| wm.checkpoint_hi)
        .expect("last item checkpoint_hi");
    assert!(terminal.checkpoint_hi.is_some_and(|hi| hi >= last_item_hi));

    let cursor = terminal.cursor.clone().expect("terminal cursor");
    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options_after(100, cursor));
    let resumed = list_checkpoints_result(&mut client, req).await;
    assert!(resumed.checkpoints.is_empty());

    let mut req = ListCheckpointsRequest::default();
    req.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    req.start_checkpoint = Some(start);
    req.end_checkpoint = Some(end);
    req.filter = Some(tx_sender(sender));
    req.options = Some(query_options(2));
    let limited = list_checkpoints_result(&mut client, req).await;
    assert_item_limit_end(limited.end, limited.end_reason);
    assert!(limited.watermarks.is_empty());
}
