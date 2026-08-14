// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Characterization ("pin") tests for the ledger-history list endpoints on the
//! fullnode (sui-rpc-api) backend, driven through the public `LedgerService`
//! trait implementation on `RpcService` over an in-memory `RpcStateReader`.
//! Inputs only: every expected output is an insta snapshot recorded from the
//! merge-base revision, then replayed on the refactor branch. Snapshot diffs
//! enumerate wire-behavior deltas.

use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt;
use prost::bytes::Bytes;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::ListCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2::ListEventsRequest;
use sui_rpc::proto::sui::rpc::v2::ListTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2::Ordering;
use sui_rpc::proto::sui::rpc::v2::QueryEnd;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::QueryOptions as ProtoQueryOptions;
use sui_rpc::proto::sui::rpc::v2::Watermark;
use sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerService;
use sui_rpc_api::RpcService;
use sui_rpc_cursor::{CursorKind, CursorToken, Position};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::committee::{Committee, EpochId};
use sui_types::digests::{CheckpointContentsDigest, CheckpointDigest, TransactionDigest};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::SuiResult;
use sui_types::event::Event;
use sui_types::full_checkpoint_content::Checkpoint as FixtureCheckpoint;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, VerifiedCheckpoint,
    VersionedFullCheckpointContents,
};
use sui_types::object::Object;
use sui_types::storage::error::Result as StorageResult;
use sui_types::storage::{
    BackingPackageStore, BalanceInfo, BalanceIterator, CoinInfo, DynamicFieldKey,
    LedgerBitmapBucketIter, LedgerBitmapBucketIterator, LedgerTxSeqDigest,
    LedgerTxSeqDigestIterator, ObjectKey, ObjectStore, OwnedObjectInfo, PackageObject, ReadStore,
    RpcIndexes, RpcStateReader, RuntimeObjectResolver,
};
use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
use sui_types::transaction::{Transaction, VerifiedTransaction};

fn test_event(n: u8) -> Event {
    use move_core_types::account_address::AccountAddress;
    use move_core_types::ident_str;
    use move_core_types::language_storage::StructTag;
    Event::new(
        &AccountAddress::TWO,
        ident_str!("pin"),
        SuiAddress::ZERO,
        StructTag {
            address: AccountAddress::TWO,
            module: move_core_types::identifier::Identifier::new("pin").unwrap(),
            name: move_core_types::identifier::Identifier::new("PinEvent").unwrap(),
            type_params: vec![],
        },
        vec![n],
    )
}

/// The shared fixture ledger:
///   cp0: tx0 (0 events)                       tx_seq 0
///   cp1: tx1 (2 ev), tx2 (0 ev), tx3 (1 ev)   tx_seqs 1,2,3
///   cp2: (no transactions)
///   cp3: tx4 (3 ev), tx5 (0 ev)               tx_seqs 4,5
///   cp4: tx6 (1 ev), tx7 (2 ev)               tx_seqs 6,7
/// Events (cp,tx_seq,ev): (1,1,0) (1,1,1) (1,3,0) (3,4,0) (3,4,1) (3,4,2)
///                        (4,6,0) (4,7,0) (4,7,1)
fn fixture_checkpoints() -> Vec<FixtureCheckpoint> {
    let mut builder = TestCheckpointBuilder::new(0);
    let mut checkpoints = Vec::new();
    let mut ev_counter = 0u8;
    let mut evs = |n: usize| -> Vec<Event> {
        (0..n)
            .map(|_| {
                ev_counter += 1;
                test_event(ev_counter)
            })
            .collect()
    };

    // cp0: one tx, no events
    builder = builder
        .start_transaction(0)
        .create_owned_object(0)
        .finish_transaction();
    checkpoints.push(builder.build_checkpoint());

    // cp1: three txs with 2 / 0 / 1 events
    builder = builder
        .start_transaction(1)
        .create_owned_object(1)
        .with_events(evs(2))
        .finish_transaction()
        .start_transaction(2)
        .create_owned_object(2)
        .finish_transaction()
        .start_transaction(3)
        .create_owned_object(3)
        .with_events(evs(1))
        .finish_transaction();
    checkpoints.push(builder.build_checkpoint());

    // cp2: empty checkpoint
    checkpoints.push(builder.build_checkpoint());

    // cp3: two txs with 3 / 0 events
    builder = builder
        .start_transaction(4)
        .create_owned_object(4)
        .with_events(evs(3))
        .finish_transaction()
        .start_transaction(5)
        .create_owned_object(5)
        .finish_transaction();
    checkpoints.push(builder.build_checkpoint());

    // cp4: two txs with 1 / 2 events
    builder = builder
        .start_transaction(6)
        .create_owned_object(6)
        .with_events(evs(1))
        .finish_transaction()
        .start_transaction(7)
        .create_owned_object(7)
        .with_events(evs(2))
        .finish_transaction();
    checkpoints.push(builder.build_checkpoint());

    checkpoints
}

// ---------------------------------------------------------------------------
// In-memory RpcStateReader + RpcIndexes over the fixture ledger
// ---------------------------------------------------------------------------

struct TxRecord {
    transaction: Arc<VerifiedTransaction>,
    effects: TransactionEffects,
    events: Option<TransactionEvents>,
    checkpoint: CheckpointSequenceNumber,
}

struct PinStore {
    summaries: Vec<VerifiedCheckpoint>,
    contents: Vec<CheckpointContents>,
    transactions: HashMap<TransactionDigest, TxRecord>,
    /// Global tx-seq ordered index rows.
    tx_rows: Vec<LedgerTxSeqDigest>,
    objects: HashMap<(ObjectID, SequenceNumber), Object>,
    latest_objects: HashMap<ObjectID, Object>,
    committee: Committee,
    lowest_available_checkpoint: CheckpointSequenceNumber,
}

impl PinStore {
    fn new(
        checkpoints: &[FixtureCheckpoint],
        lowest_available_checkpoint: CheckpointSequenceNumber,
    ) -> Self {
        let mut summaries = Vec::new();
        let mut contents = Vec::new();
        let mut transactions = HashMap::new();
        let mut tx_rows = Vec::new();
        let mut objects = HashMap::new();
        let mut latest_objects: HashMap<ObjectID, Object> = HashMap::new();
        for checkpoint in checkpoints {
            let summary = checkpoint.summary.clone();
            let checkpoint_number = summary.data().sequence_number;
            let first_tx_seq = summary
                .data()
                .network_total_transactions
                .checked_sub(checkpoint.transactions.len() as u64)
                .expect("checkpoint transaction range");
            for (offset, transaction) in checkpoint.transactions.iter().enumerate() {
                let tx = Transaction::from_generic_sig_data(
                    transaction.transaction.clone(),
                    transaction.signatures.clone(),
                );
                let digest = *tx.digest();
                let event_count = transaction
                    .events
                    .as_ref()
                    .map_or(0, |events| events.data.len() as u32);
                tx_rows.push(LedgerTxSeqDigest {
                    tx_sequence_number: first_tx_seq + offset as u64,
                    digest,
                    event_count,
                    tx_offset: offset as u32,
                    checkpoint_number,
                });
                transactions.insert(
                    digest,
                    TxRecord {
                        transaction: Arc::new(VerifiedTransaction::new_unchecked(tx)),
                        effects: transaction.effects.clone(),
                        events: transaction.events.clone(),
                        checkpoint: checkpoint_number,
                    },
                );
            }
            for object in checkpoint.object_set.iter() {
                objects.insert((object.id(), object.version()), object.clone());
                let slot = latest_objects.entry(object.id()).or_insert(object.clone());
                if slot.version() < object.version() {
                    *slot = object.clone();
                }
            }
            summaries.push(VerifiedCheckpoint::new_unchecked(summary));
            contents.push(checkpoint.contents.clone());
        }
        let (committee, _) = Committee::new_simple_test_committee();
        Self {
            summaries,
            contents,
            transactions,
            tx_rows,
            objects,
            latest_objects,
            committee,
            lowest_available_checkpoint,
        }
    }
}

impl ObjectStore for PinStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.latest_objects.get(object_id).cloned()
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.objects.get(&(*object_id, version)).cloned()
    }
}

impl ReadStore for PinStore {
    fn get_committee(&self, _epoch: EpochId) -> Option<Arc<Committee>> {
        Some(Arc::new(self.committee.clone()))
    }

    fn get_latest_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        Ok(self.summaries.last().expect("fixture is non-empty").clone())
    }

    fn get_highest_verified_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        self.get_latest_checkpoint()
    }

    fn get_highest_synced_checkpoint(&self) -> StorageResult<VerifiedCheckpoint> {
        self.get_latest_checkpoint()
    }

    fn get_lowest_available_checkpoint(&self) -> StorageResult<CheckpointSequenceNumber> {
        Ok(self.lowest_available_checkpoint)
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.summaries
            .iter()
            .find(|summary| summary.digest() == digest)
            .cloned()
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.summaries.get(sequence_number as usize).cloned()
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.contents
            .iter()
            .find(|contents| contents.digest() == digest)
            .cloned()
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        self.contents.get(sequence_number as usize).cloned()
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.transactions
            .get(digest)
            .map(|record| record.transaction.clone())
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.transactions
            .get(digest)
            .map(|record| record.effects.clone())
    }

    fn get_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.transactions
            .get(digest)
            .and_then(|record| record.events.clone())
    }

    fn get_unchanged_loaded_runtime_objects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<Vec<ObjectKey>> {
        self.transactions.get(digest).map(|_| Vec::new())
    }

    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        self.transactions
            .get(digest)
            .map(|record| record.checkpoint)
    }

    fn get_full_checkpoint_contents(
        &self,
        _sequence_number: Option<CheckpointSequenceNumber>,
        _digest: &CheckpointContentsDigest,
    ) -> Option<VersionedFullCheckpointContents> {
        None
    }
}

impl BackingPackageStore for PinStore {
    fn get_package_object(&self, _package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        Ok(None)
    }
}

impl RuntimeObjectResolver for PinStore {
    fn read_child_object(
        &self,
        _parent: &ObjectID,
        _child: &ObjectID,
        _child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(None)
    }

    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        _receiving_object_id: &ObjectID,
        _receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        Ok(None)
    }
}

impl RpcStateReader for PinStore {
    fn get_lowest_available_checkpoint_objects(
        &self,
    ) -> StorageResult<CheckpointSequenceNumber> {
        Ok(self.lowest_available_checkpoint)
    }

    fn get_chain_identifier(&self) -> StorageResult<sui_types::digests::ChainIdentifier> {
        Ok((*self.summaries[0].digest()).into())
    }

    fn indexes(&self) -> Option<&dyn RpcIndexes> {
        Some(self)
    }

    fn get_struct_layout_with_overlay(
        &self,
        _struct_tag: &move_core_types::language_storage::StructTag,
        _overlay: &ObjectSet,
    ) -> StorageResult<Option<move_core_types::annotated_value::MoveTypeLayout>> {
        Ok(None)
    }
}

/// Empty seekable bitmap-bucket iterator: unfiltered pin scenarios never touch
/// the bitmap indexes, and filtered ones are out of scope for this suite.
struct NoBuckets;

type BucketItem = <LedgerBitmapBucketIterator<'static> as Iterator>::Item;

impl Iterator for NoBuckets {
    type Item = BucketItem;
    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}

impl LedgerBitmapBucketIter for NoBuckets {
    fn seek_bucket(&mut self, _bucket_id: u64) {}
}

impl RpcIndexes for PinStore {
    fn get_epoch_info(
        &self,
        _epoch: EpochId,
    ) -> StorageResult<Option<sui_types::storage::EpochInfo>> {
        unimplemented!("pin-test mock: get_epoch_info")
    }

    fn owned_objects_iter(
        &self,
        _owner: SuiAddress,
        _object_type: Option<move_core_types::language_storage::StructTag>,
        _cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<
        Box<dyn Iterator<Item = Result<OwnedObjectInfo, typed_store_error::TypedStoreError>> + '_>,
    > {
        unimplemented!("pin-test mock: owned_objects_iter")
    }

    fn dynamic_field_iter(
        &self,
        _parent: ObjectID,
        _cursor: Option<DynamicFieldKey>,
    ) -> StorageResult<
        Box<dyn Iterator<Item = sui_types::storage::DynamicFieldIteratorItem> + '_>,
    > {
        unimplemented!("pin-test mock: dynamic_field_iter")
    }

    fn get_coin_info(
        &self,
        _coin_type: &move_core_types::language_storage::StructTag,
    ) -> StorageResult<Option<CoinInfo>> {
        unimplemented!("pin-test mock: get_coin_info")
    }

    fn get_balance(
        &self,
        _owner: &SuiAddress,
        _coin_type: &move_core_types::language_storage::StructTag,
    ) -> StorageResult<Option<BalanceInfo>> {
        unimplemented!("pin-test mock: get_balance")
    }

    fn balance_iter(
        &self,
        _owner: &SuiAddress,
        _cursor: Option<(SuiAddress, move_core_types::language_storage::StructTag)>,
    ) -> StorageResult<BalanceIterator<'_>> {
        unimplemented!("pin-test mock: balance_iter")
    }

    fn package_versions_iter(
        &self,
        _original_id: ObjectID,
        _cursor: Option<u64>,
    ) -> StorageResult<
        Box<dyn Iterator<Item = Result<(u64, ObjectID), typed_store_error::TypedStoreError>> + '_>,
    > {
        unimplemented!("pin-test mock: package_versions_iter")
    }

    fn get_highest_indexed_checkpoint_seq_number(
        &self,
    ) -> StorageResult<Option<CheckpointSequenceNumber>> {
        Ok(self.summaries.last().map(|summary| *summary.sequence_number()))
    }

    fn ledger_tx_seq_digest(&self, tx_seq: u64) -> StorageResult<Option<LedgerTxSeqDigest>> {
        Ok(self.tx_rows.get(tx_seq as usize).cloned())
    }

    fn ledger_tx_seq_digest_iter(
        &self,
        start: u64,
        end_exclusive: u64,
        descending: bool,
    ) -> StorageResult<LedgerTxSeqDigestIterator<'_>> {
        let rows: Vec<LedgerTxSeqDigest> = self
            .tx_rows
            .iter()
            .filter(|row| row.tx_sequence_number >= start && row.tx_sequence_number < end_exclusive)
            .cloned()
            .collect();
        let rows: Box<dyn Iterator<Item = LedgerTxSeqDigest>> = if descending {
            Box::new(rows.into_iter().rev())
        } else {
            Box::new(rows.into_iter())
        };
        Ok(Box::new(rows.map(Ok)))
    }

    fn transaction_bitmap_bucket_iter(
        &self,
        _dimension_key: Vec<u8>,
        _start_bucket: u64,
        _end_bucket_exclusive: u64,
        _descending: bool,
    ) -> StorageResult<LedgerBitmapBucketIterator<'_>> {
        Ok(Box::new(NoBuckets))
    }

    fn event_bitmap_bucket_iter(
        &self,
        _dimension_key: Vec<u8>,
        _start_bucket: u64,
        _end_bucket_exclusive: u64,
        _descending: bool,
    ) -> StorageResult<LedgerBitmapBucketIterator<'_>> {
        Ok(Box::new(NoBuckets))
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Harness {
    service: RpcService,
    digest_to_seq: HashMap<String, u64>,
}

fn harness_with_floor(lowest_available_checkpoint: u64) -> Harness {
    let checkpoints = fixture_checkpoints();
    let mut digest_to_seq = HashMap::new();
    let mut seq = 0u64;
    for checkpoint in &checkpoints {
        for transaction in &checkpoint.transactions {
            let tx = Transaction::from_generic_sig_data(
                transaction.transaction.clone(),
                transaction.signatures.clone(),
            );
            digest_to_seq.insert(tx.digest().to_string(), seq);
            seq += 1;
        }
    }
    let store = PinStore::new(&checkpoints, lowest_available_checkpoint);
    let service = RpcService::new(Arc::new(store));
    Harness {
        service,
        digest_to_seq,
    }
}

fn harness() -> Harness {
    harness_with_floor(0)
}

// ---------------------------------------------------------------------------
// Cursor / request helpers
// ---------------------------------------------------------------------------

fn cp_pos(checkpoint: u64) -> Position {
    Position::Checkpoints { checkpoint }
}

fn tx_pos(checkpoint: u64, tx_seq: u64) -> Position {
    Position::Transactions { checkpoint, tx_seq }
}

fn ev_pos(checkpoint: u64, tx_seq: u64, event_index: u32) -> Position {
    Position::Events {
        checkpoint,
        tx_seq,
        event_index,
    }
}

fn item(position: Position) -> Bytes {
    CursorToken::item(position).encode()
}

fn bnd(position: Position) -> Bytes {
    CursorToken::boundary(position).encode()
}

#[derive(Default, Clone)]
struct Opts {
    limit: Option<u32>,
    desc: bool,
    after: Option<Bytes>,
    before: Option<Bytes>,
}

impl Opts {
    fn to_proto(&self) -> ProtoQueryOptions {
        let mut options = ProtoQueryOptions::default();
        options.limit = self.limit;
        if self.desc {
            options.ordering = Some(Ordering::Descending as i32);
        }
        options.after = self.after.clone();
        options.before = self.before.clone();
        options
    }

    fn after(mut self, cursor: Bytes) -> Self {
        self.after = Some(cursor);
        self
    }

    fn before(mut self, cursor: Bytes) -> Self {
        self.before = Some(cursor);
        self
    }

    fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
}

fn asc() -> Opts {
    Opts::default()
}

fn desc() -> Opts {
    Opts {
        desc: true,
        ..Opts::default()
    }
}

fn cp_req(start: Option<u64>, end: Option<u64>, opts: Opts) -> ListCheckpointsRequest {
    let mut request = ListCheckpointsRequest::default();
    request.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    request.start_checkpoint = start;
    request.end_checkpoint = end;
    request.options = Some(opts.to_proto());
    request
}

fn tx_req(start: Option<u64>, end: Option<u64>, opts: Opts) -> ListTransactionsRequest {
    let mut request = ListTransactionsRequest::default();
    request.read_mask = Some(FieldMask::from_paths(["digest"]));
    request.start_checkpoint = start;
    request.end_checkpoint = end;
    request.options = Some(opts.to_proto());
    request
}

fn ev_req(start: Option<u64>, end: Option<u64>, opts: Opts) -> ListEventsRequest {
    let mut request = ListEventsRequest::default();
    request.read_mask = Some(FieldMask::from_paths([
        "checkpoint",
        "transaction_index",
        "event_index",
    ]));
    request.start_checkpoint = start;
    request.end_checkpoint = end;
    request.options = Some(opts.to_proto());
    request
}

// ---------------------------------------------------------------------------
// Response formatting
// ---------------------------------------------------------------------------

fn fmt_cursor(bytes: &[u8]) -> String {
    match CursorToken::decode(bytes) {
        Ok(token) => {
            let kind = match token.kind {
                CursorKind::Item => "item",
                CursorKind::Boundary => "boundary",
            };
            match token.position {
                Position::Checkpoints { checkpoint } => format!("{kind}(cp={checkpoint})"),
                Position::Transactions { checkpoint, tx_seq } => {
                    format!("{kind}(cp={checkpoint},tx={tx_seq})")
                }
                Position::Events {
                    checkpoint,
                    tx_seq,
                    event_index,
                } => format!("{kind}(cp={checkpoint},tx={tx_seq},ev={event_index})"),
            }
        }
        Err(error) => format!("<undecodable: {error}>"),
    }
}

fn fmt_watermark(watermark: Option<&Watermark>) -> String {
    match watermark {
        None => "wm=NONE".to_string(),
        Some(watermark) => {
            let cursor = watermark
                .cursor
                .as_deref()
                .map(fmt_cursor)
                .unwrap_or_else(|| "-".to_string());
            let checkpoint = watermark
                .checkpoint
                .map(|checkpoint| checkpoint.to_string())
                .unwrap_or_else(|| "-".to_string());
            format!("wm.cursor={cursor} wm.cp={checkpoint}")
        }
    }
}

fn fmt_end(end: Option<&QueryEnd>) -> String {
    match end {
        None => "end=-".to_string(),
        Some(end) => format!(
            "end={}",
            QueryEndReason::try_from(end.reason.unwrap_or_default())
                .map(|reason| reason.as_str_name().to_string())
                .unwrap_or_else(|_| format!("<raw {}>", end.reason.unwrap_or_default()))
        ),
    }
}

fn fmt_status(status: tonic::Status) -> String {
    format!("ERROR code={:?} msg={:?}", status.code(), status.message())
}

// ---------------------------------------------------------------------------
// Scenario runners (one per endpoint)
// ---------------------------------------------------------------------------

async fn run_checkpoints(service: &RpcService, request: ListCheckpointsRequest) -> String {
    let mut out = Vec::new();
    match LedgerService::list_checkpoints(service, tonic::Request::new(request)).await {
        Err(status) => out.push(format!("INIT {}", fmt_status(status))),
        Ok(response) => {
            let frames: Vec<_> = response.into_inner().collect().await;
            for (index, frame) in frames.into_iter().enumerate() {
                match frame {
                    Err(status) => out.push(format!("#{index} {}", fmt_status(status))),
                    Ok(response) => {
                        let payload = response
                            .checkpoint
                            .as_ref()
                            .map(|checkpoint| {
                                format!(
                                    "item cp={}",
                                    checkpoint
                                        .sequence_number
                                        .map(|n| n.to_string())
                                        .unwrap_or_else(|| "?".to_string())
                                )
                            })
                            .unwrap_or_else(|| "no-item".to_string());
                        out.push(format!(
                            "#{index} {payload} | {} | {}",
                            fmt_watermark(response.watermark.as_ref()),
                            fmt_end(response.end.as_ref()),
                        ));
                    }
                }
            }
        }
    }
    out.join("\n")
}

async fn run_transactions(harness: &Harness, request: ListTransactionsRequest) -> String {
    let mut out = Vec::new();
    match LedgerService::list_transactions(&harness.service, tonic::Request::new(request)).await {
        Err(status) => out.push(format!("INIT {}", fmt_status(status))),
        Ok(response) => {
            let frames: Vec<_> = response.into_inner().collect().await;
            for (index, frame) in frames.into_iter().enumerate() {
                match frame {
                    Err(status) => out.push(format!("#{index} {}", fmt_status(status))),
                    Ok(response) => {
                        let payload = response
                            .transaction
                            .as_ref()
                            .map(|transaction| {
                                let digest = transaction.digest.clone().unwrap_or_default();
                                match harness.digest_to_seq.get(&digest) {
                                    Some(seq) => format!("item tx_seq={seq}"),
                                    None => format!("item digest={digest}"),
                                }
                            })
                            .unwrap_or_else(|| "no-item".to_string());
                        out.push(format!(
                            "#{index} {payload} | {} | {}",
                            fmt_watermark(response.watermark.as_ref()),
                            fmt_end(response.end.as_ref()),
                        ));
                    }
                }
            }
        }
    }
    out.join("\n")
}

async fn run_events(service: &RpcService, request: ListEventsRequest) -> String {
    let mut out = Vec::new();
    match LedgerService::list_events(service, tonic::Request::new(request)).await {
        Err(status) => out.push(format!("INIT {}", fmt_status(status))),
        Ok(response) => {
            let frames: Vec<_> = response.into_inner().collect().await;
            for (index, frame) in frames.into_iter().enumerate() {
                match frame {
                    Err(status) => out.push(format!("#{index} {}", fmt_status(status))),
                    Ok(response) => {
                        let payload = response
                            .event
                            .as_ref()
                            .map(|event| {
                                format!(
                                    "item ev=(cp={},tx_idx={},ev={})",
                                    event
                                        .checkpoint
                                        .map(|n| n.to_string())
                                        .unwrap_or_else(|| "?".to_string()),
                                    event
                                        .transaction_index
                                        .map(|n| n.to_string())
                                        .unwrap_or_else(|| "?".to_string()),
                                    event
                                        .event_index
                                        .map(|n| n.to_string())
                                        .unwrap_or_else(|| "?".to_string()),
                                )
                            })
                            .unwrap_or_else(|| "no-item".to_string());
                        out.push(format!(
                            "#{index} {payload} | {} | {}",
                            fmt_watermark(response.watermark.as_ref()),
                            fmt_end(response.end.as_ref()),
                        ));
                    }
                }
            }
        }
    }
    out.join("\n")
}

// ---------------------------------------------------------------------------
// Scenario matrices (identical to the kv backend's pin suite)
// ---------------------------------------------------------------------------

fn checkpoint_scenarios() -> Vec<(&'static str, ListCheckpointsRequest)> {
    vec![
        ("no_bounds_asc", cp_req(None, None, asc())),
        ("no_bounds_desc", cp_req(None, None, desc())),
        ("window_1_4_asc", cp_req(Some(1), Some(4), asc())),
        ("window_1_4_desc", cp_req(Some(1), Some(4), desc())),
        ("start_eq_end_asc", cp_req(Some(3), Some(3), asc())),
        ("end_lt_start", cp_req(Some(3), Some(2), asc())),
        ("end_beyond_tip_asc", cp_req(Some(3), Some(100), asc())),
        ("end_beyond_tip_desc", cp_req(Some(3), Some(100), desc())),
        ("start_beyond_tip_asc", cp_req(Some(100), None, asc())),
        ("start_at_last_cp_asc", cp_req(Some(4), None, asc())),
        ("end_eq_tip_asc", cp_req(None, Some(5), asc())),
        ("after_item_mid_asc", cp_req(None, None, asc().after(item(cp_pos(2))))),
        (
            "after_boundary_mid_asc",
            cp_req(None, None, asc().after(bnd(cp_pos(2)))),
        ),
        (
            "after_item_last_asc",
            cp_req(None, None, asc().after(item(cp_pos(4)))),
        ),
        (
            "after_boundary_last_asc",
            cp_req(None, None, asc().after(bnd(cp_pos(4)))),
        ),
        (
            "after_beyond_tip_asc",
            cp_req(None, None, asc().after(item(cp_pos(100)))),
        ),
        (
            "before_item_mid_desc",
            cp_req(None, None, desc().before(item(cp_pos(2)))),
        ),
        (
            "before_boundary_mid_desc",
            cp_req(None, None, desc().before(bnd(cp_pos(2)))),
        ),
        (
            "before_first_desc",
            cp_req(None, None, desc().before(item(cp_pos(0)))),
        ),
        (
            "before_beyond_tip_desc",
            cp_req(None, None, desc().before(item(cp_pos(100)))),
        ),
        (
            "after_before_adjacent_asc",
            cp_req(None, None, asc().after(item(cp_pos(1))).before(item(cp_pos(2)))),
        ),
        (
            "after_ge_before_asc",
            cp_req(None, None, asc().after(item(cp_pos(3))).before(item(cp_pos(2)))),
        ),
        (
            "window_after_collapse_asc",
            cp_req(Some(1), Some(3), asc().after(item(cp_pos(4)))),
        ),
        (
            "window_before_collapse_desc",
            cp_req(Some(2), Some(4), desc().before(item(cp_pos(0)))),
        ),
        (
            "after_below_window_asc",
            cp_req(Some(2), Some(4), asc().after(item(cp_pos(0)))),
        ),
        (
            "after_at_window_end_asc",
            cp_req(Some(1), Some(3), asc().after(item(cp_pos(2)))),
        ),
        (
            "before_at_window_start_desc",
            cp_req(Some(1), Some(3), desc().before(item(cp_pos(1)))),
        ),
        (
            "after_bound_desc",
            cp_req(None, None, desc().after(item(cp_pos(2)))),
        ),
        (
            "before_bound_asc",
            cp_req(None, None, asc().before(item(cp_pos(3)))),
        ),
    ]
}

fn transaction_scenarios() -> Vec<(&'static str, ListTransactionsRequest)> {
    vec![
        ("no_bounds_asc", tx_req(None, None, asc())),
        ("no_bounds_desc", tx_req(None, None, desc())),
        ("window_1_4_asc", tx_req(Some(1), Some(4), asc())),
        ("window_1_4_desc", tx_req(Some(1), Some(4), desc())),
        ("start_eq_end_asc", tx_req(Some(3), Some(3), asc())),
        ("end_lt_start", tx_req(Some(3), Some(2), asc())),
        ("end_beyond_tip_asc", tx_req(Some(1), Some(100), asc())),
        ("start_beyond_tip_asc", tx_req(Some(100), None, asc())),
        ("empty_cp_window_asc", tx_req(Some(2), Some(3), asc())),
        ("empty_cp_window_desc", tx_req(Some(2), Some(3), desc())),
        (
            "after_item_mid_asc",
            tx_req(None, None, asc().after(item(tx_pos(1, 2)))),
        ),
        (
            "after_boundary_mid_asc",
            tx_req(None, None, asc().after(bnd(tx_pos(1, 2)))),
        ),
        (
            "after_item_cp_edge_asc",
            tx_req(None, None, asc().after(item(tx_pos(1, 3)))),
        ),
        (
            "after_item_last_asc",
            tx_req(None, None, asc().after(item(tx_pos(4, 7)))),
        ),
        (
            "after_beyond_tip_asc",
            tx_req(None, None, asc().after(item(tx_pos(100, 100)))),
        ),
        (
            "before_item_mid_desc",
            tx_req(None, None, desc().before(item(tx_pos(3, 5)))),
        ),
        (
            "before_boundary_mid_desc",
            tx_req(None, None, desc().before(bnd(tx_pos(3, 5)))),
        ),
        (
            "before_first_desc",
            tx_req(None, None, desc().before(item(tx_pos(0, 0)))),
        ),
        (
            "before_cp_first_tx_desc",
            tx_req(None, None, desc().before(item(tx_pos(3, 4)))),
        ),
        (
            "after_before_adjacent_asc",
            tx_req(
                None,
                None,
                asc().after(item(tx_pos(1, 2))).before(item(tx_pos(1, 3))),
            ),
        ),
        (
            "after_ge_before_asc",
            tx_req(
                None,
                None,
                asc().after(item(tx_pos(3, 5))).before(item(tx_pos(1, 2))),
            ),
        ),
        (
            "window_after_collapse_asc",
            tx_req(Some(1), Some(3), asc().after(item(tx_pos(4, 6)))),
        ),
        (
            "window_before_collapse_desc",
            tx_req(Some(3), Some(5), desc().before(item(tx_pos(0, 0)))),
        ),
        (
            "after_bound_desc",
            tx_req(None, None, desc().after(item(tx_pos(1, 2)))),
        ),
        (
            "before_bound_asc",
            tx_req(None, None, asc().before(item(tx_pos(3, 5)))),
        ),
    ]
}

fn event_scenarios() -> Vec<(&'static str, ListEventsRequest)> {
    vec![
        ("no_bounds_asc", ev_req(None, None, asc())),
        ("no_bounds_desc", ev_req(None, None, desc())),
        ("window_1_4_asc", ev_req(Some(1), Some(4), asc())),
        ("window_1_4_desc", ev_req(Some(1), Some(4), desc())),
        ("start_eq_end_asc", ev_req(Some(3), Some(3), asc())),
        ("end_lt_start", ev_req(Some(3), Some(2), asc())),
        ("end_beyond_tip_asc", ev_req(Some(1), Some(100), asc())),
        ("start_beyond_tip_asc", ev_req(Some(100), None, asc())),
        ("no_event_cp_window_asc", ev_req(Some(2), Some(3), asc())),
        ("no_event_cp_window_desc", ev_req(Some(2), Some(3), desc())),
        (
            "after_mid_tx_asc",
            ev_req(None, None, asc().after(item(ev_pos(1, 1, 0)))),
        ),
        (
            "after_last_ev_of_tx_asc",
            ev_req(None, None, asc().after(item(ev_pos(1, 1, 1)))),
        ),
        (
            "after_boundary_mid_tx_asc",
            ev_req(None, None, asc().after(bnd(ev_pos(1, 1, 1)))),
        ),
        (
            "after_eventless_tx_pos_asc",
            ev_req(None, None, asc().after(item(ev_pos(1, 2, 0)))),
        ),
        (
            "after_last_event_asc",
            ev_req(None, None, asc().after(item(ev_pos(4, 7, 1)))),
        ),
        (
            "after_beyond_tip_asc",
            ev_req(None, None, asc().after(item(ev_pos(100, 100, 0)))),
        ),
        (
            "after_ev_index_past_tx_asc",
            ev_req(None, None, asc().after(item(ev_pos(1, 1, 5)))),
        ),
        (
            "boundary_at_zero_asc",
            ev_req(None, None, asc().after(bnd(ev_pos(0, 0, 0)))),
        ),
        (
            "before_mid_tx_desc",
            ev_req(None, None, desc().before(item(ev_pos(3, 4, 1)))),
        ),
        (
            "before_boundary_mid_tx_desc",
            ev_req(None, None, desc().before(bnd(ev_pos(3, 4, 1)))),
        ),
        (
            "before_first_event_desc",
            ev_req(None, None, desc().before(item(ev_pos(1, 1, 0)))),
        ),
        (
            "before_tx_first_ev_desc",
            ev_req(None, None, desc().before(item(ev_pos(3, 4, 0)))),
        ),
        (
            "after_before_adjacent_asc",
            ev_req(
                None,
                None,
                asc().after(item(ev_pos(3, 4, 0))).before(item(ev_pos(3, 4, 1))),
            ),
        ),
        (
            "after_ge_before_asc",
            ev_req(
                None,
                None,
                asc().after(item(ev_pos(4, 6, 0))).before(item(ev_pos(1, 1, 0))),
            ),
        ),
        (
            "window_after_collapse_asc",
            ev_req(Some(1), Some(3), asc().after(item(ev_pos(4, 6, 0)))),
        ),
        (
            "window_before_collapse_desc",
            ev_req(Some(3), Some(5), desc().before(item(ev_pos(0, 0, 0)))),
        ),
        (
            "after_bound_desc",
            ev_req(None, None, desc().after(item(ev_pos(1, 1, 0)))),
        ),
        (
            "before_bound_asc",
            ev_req(None, None, asc().before(item(ev_pos(3, 4, 1)))),
        ),
    ]
}

// ---------------------------------------------------------------------------
// Matrix tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn pin_checkpoints_matrix() {
    let harness = harness();
    for (name, request) in checkpoint_scenarios() {
        let output = run_checkpoints(&harness.service, request).await;
        insta::assert_snapshot!(format!("checkpoints__{name}"), output);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pin_transactions_matrix() {
    let harness = harness();
    for (name, request) in transaction_scenarios() {
        let output = run_transactions(&harness, request).await;
        insta::assert_snapshot!(format!("transactions__{name}"), output);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pin_events_matrix() {
    let harness = harness();
    for (name, request) in event_scenarios() {
        let output = run_events(&harness.service, request).await;
        insta::assert_snapshot!(format!("events__{name}"), output);
    }
}

// ---------------------------------------------------------------------------
// Serving-floor (pruning) scenarios — fullnode backend only: the kv backend's
// unit harness has no pruning floor concept.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn pin_pruned_floor_matrix() {
    let harness = harness_with_floor(2);
    for (name, request) in [
        ("checkpoints_no_bounds_asc", cp_req(None, None, asc())),
        ("checkpoints_start_below_floor", cp_req(Some(0), None, asc())),
        (
            "checkpoints_after_below_floor",
            cp_req(None, None, asc().after(item(cp_pos(0)))),
        ),
    ] {
        let output = run_checkpoints(&harness.service, request).await;
        insta::assert_snapshot!(format!("pruned__{name}"), output);
    }
    for (name, request) in [
        ("transactions_no_bounds_asc", tx_req(None, None, asc())),
        ("transactions_start_below_floor", tx_req(Some(0), None, asc())),
        (
            "transactions_after_below_floor",
            tx_req(None, None, asc().after(item(tx_pos(0, 0)))),
        ),
    ] {
        let output = run_transactions(&harness, request).await;
        insta::assert_snapshot!(format!("pruned__{name}"), output);
    }
    for (name, request) in [
        ("events_no_bounds_asc", ev_req(None, None, asc())),
        (
            "events_after_below_floor",
            ev_req(None, None, asc().after(item(ev_pos(0, 0, 0)))),
        ),
        ("events_no_bounds_desc", ev_req(None, None, desc())),
    ] {
        let output = run_events(&harness.service, request).await;
        insta::assert_snapshot!(format!("pruned__{name}"), output);
    }
}

// ---------------------------------------------------------------------------
// Pagination roundtrip walks
// ---------------------------------------------------------------------------

/// Re-encode the final frame's `wm.cursor=` rendering back into cursor bytes.
/// Only the formats produced by `fmt_cursor` are accepted.
fn parse_last_cursor(transcript: &str) -> Bytes {
    let line = transcript
        .lines()
        .rev()
        .find(|line| line.contains("wm.cursor="))
        .expect("terminal frame carries a watermark cursor");
    let start = line.find("wm.cursor=").unwrap() + "wm.cursor=".len();
    let rest = &line[start..];
    let end = rest.find(" wm.cp=").expect("cursor rendering is delimited");
    let rendered = &rest[..end];
    let (kind, coords) = rendered
        .split_once('(')
        .expect("cursor rendering has kind(coords)");
    let coords = coords.strip_suffix(')').expect("cursor rendering closes");
    let mut cp = None;
    let mut tx = None;
    let mut ev = None;
    for part in coords.split(',') {
        let (key, value) = part.split_once('=').expect("coord is key=value");
        let value: u64 = value.parse().expect("coord value is numeric");
        match key {
            "cp" => cp = Some(value),
            "tx" => tx = Some(value),
            "ev" => ev = Some(value),
            other => panic!("unexpected coord key {other}"),
        }
    }
    let position = match (cp, tx, ev) {
        (Some(cp), None, None) => Position::Checkpoints { checkpoint: cp },
        (Some(cp), Some(tx), None) => Position::Transactions {
            checkpoint: cp,
            tx_seq: tx,
        },
        (Some(cp), Some(tx), Some(ev)) => Position::Events {
            checkpoint: cp,
            tx_seq: tx,
            event_index: ev as u32,
        },
        other => panic!("unexpected coord combination {other:?}"),
    };
    match kind {
        "item" => CursorToken::item(position).encode(),
        "boundary" => CursorToken::boundary(position).encode(),
        other => panic!("unexpected cursor kind {other}"),
    }
}

macro_rules! walk {
    ($run:expr, $req:expr, $descending:expr, $limit:expr) => {{
        let mut pages = Vec::new();
        let mut cursor: Option<Bytes> = None;
        for page in 0..40 {
            let mut opts = if $descending { desc() } else { asc() }.limit($limit);
            if let Some(cursor) = cursor.clone() {
                if $descending {
                    opts = opts.before(cursor);
                } else {
                    opts = opts.after(cursor);
                }
            }
            let request = $req(opts);
            let output = $run(request).await;
            pages.push(format!("== page {page} ==\n{output}"));
            let last = pages.last().unwrap();
            let terminal_is_item_limit = last.contains("end=QUERY_END_REASON_ITEM_LIMIT");
            if !terminal_is_item_limit {
                break;
            }
            cursor = Some(parse_last_cursor(last));
        }
        pages.join("\n\n")
    }};
}

#[tokio::test(flavor = "multi_thread")]
async fn pin_checkpoints_pagination_walks() {
    let harness = harness();
    for limit in [1u32, 3] {
        for descending in [false, true] {
            let direction = if descending { "desc" } else { "asc" };
            let transcript = walk!(
                |request| run_checkpoints(&harness.service, request),
                |opts| cp_req(None, None, opts),
                descending,
                limit
            );
            insta::assert_snapshot!(
                format!("walk__checkpoints__limit{limit}_{direction}"),
                transcript
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pin_transactions_pagination_walks() {
    let harness = harness();
    for limit in [1u32, 3] {
        for descending in [false, true] {
            let direction = if descending { "desc" } else { "asc" };
            let transcript = walk!(
                |request| run_transactions(&harness, request),
                |opts| tx_req(None, None, opts),
                descending,
                limit
            );
            insta::assert_snapshot!(
                format!("walk__transactions__limit{limit}_{direction}"),
                transcript
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pin_events_pagination_walks() {
    let harness = harness();
    for limit in [1u32, 3] {
        for descending in [false, true] {
            let direction = if descending { "desc" } else { "asc" };
            let transcript = walk!(
                |request| run_events(&harness.service, request),
                |opts| ev_req(None, None, opts),
                descending,
                limit
            );
            insta::assert_snapshot!(format!("walk__events__limit{limit}_{direction}"), transcript);
        }
    }
}
