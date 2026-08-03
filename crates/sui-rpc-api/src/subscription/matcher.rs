// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! In-memory DNF filter matching for subscription streams.
//!
//! Each filtered subscriber's compiled [`BitmapQuery`] terms are registered in
//! an anchor table keyed by one chosen include-literal dimension key per term.
//! Per executed checkpoint, every transaction's (and, when an event
//! subscription exists, every event's) encoded dimension keys are extracted
//! with the same `sui-inverted-index` visitors the persistent index uses; a
//! key hit in the anchor table triggers full evaluation of that term against
//! the key set. A term can only match when its anchor include key is present,
//! and OR-of-terms means any matching term settles the filter, so term-level
//! evaluation is exact for the whole DNF.
//!
//! Unanchored negation (exclude-only terms) rides the synthesized
//! `TxUniverse` / `EventExtant` universe includes that
//! `transaction_filter_to_query` / `event_filter_to_query` already inject:
//! the tx-space pass inserts the `TxUniverse` key into every transaction's
//! key set (no write path emits it), while `EventExtant` arrives naturally
//! per real event.
//!
//! Every encoded key is hashed exactly once -- keyed, at extraction or term
//! registration -- and anchor probes and term evaluations route hash-table
//! buckets through that stored hash (see [`PrehashedKey`]).

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Arc;
use std::sync::LazyLock;

use sui_inverted_index::BitmapKey;
use sui_inverted_index::BitmapLiteral;
use sui_inverted_index::BitmapTerm;
use sui_inverted_index::IndexDimension;
use sui_inverted_index::TX_UNIVERSE_VALUE;
use sui_inverted_index::encode_dimension_key;
use sui_inverted_index::for_each_event_dimension;
use sui_inverted_index::for_each_transaction_dimension;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::transaction::TransactionDataAPI as _;
use tokio::sync::mpsc;
use tracing::trace;

use super::MatchedCheckpoint;
use super::SubscriptionKind;
use super::SubscriptionLifecycleGuard;
use super::SubscriptionMatches;
use super::SubscriptionSpec;
use super::SubscriptionTerminationReason;
use super::SubscriptionUpdate;

/// Process-global keyed seed for [`PrehashedKey`] hashes. Keyed hashing is
/// required for HashDoS resistance: key bytes derive from attacker-chosen
/// on-chain data (senders, object IDs, event types), and the passthrough
/// tables trust the stored hash for bucket routing.
static PREHASH_SEED: LazyLock<RandomState> = LazyLock::new(RandomState::new);

/// An encoded dimension key carrying its keyed hash, computed exactly once
/// (at extraction or term registration). The hash only routes buckets; byte
/// equality remains the authority for every comparison.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PrehashedKey {
    hash: u64,
    bytes: Vec<u8>,
}

impl PrehashedKey {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            hash: PREHASH_SEED.hash_one(bytes.as_slice()),
            bytes,
        }
    }
}

impl Hash for PrehashedKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

/// `BuildHasher` that finishes with the `u64` a [`PrehashedKey`] wrote,
/// instead of re-hashing key bytes.
#[derive(Clone, Copy, Default)]
pub(crate) struct Passthrough;

impl BuildHasher for Passthrough {
    type Hasher = PassthroughHasher;

    fn build_hasher(&self) -> PassthroughHasher {
        PassthroughHasher(0)
    }
}

pub(crate) struct PassthroughHasher(u64);

impl Hasher for PassthroughHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _bytes: &[u8]) {
        unreachable!("prehashed keys hash by writing their stored u64")
    }

    fn write_u64(&mut self, hash: u64) {
        self.0 = hash;
    }
}

/// Dimension-key set probed via stored hashes.
pub(crate) type KeySet = HashSet<PrehashedKey, Passthrough>;
/// Anchor table: prehashed key -> terms anchored on it.
type AnchorTable = HashMap<PrehashedKey, Vec<TermRef>, Passthrough>;

/// Per-checkpoint extracted dimension-key sets, computed once by the
/// dispatcher and shared read-only across shards.
pub(crate) struct CheckpointKeys {
    /// Per-transaction key sets (incl. the synthetic `TxUniverse` key).
    /// `None` when no filtered tx-space subscriber existed at extraction.
    pub(crate) tx: Option<Vec<KeySet>>,
    /// Per-transaction, per-event key sets; empty inner `Vec` for txs
    /// without events. `None` when no filtered event subscriber existed.
    pub(crate) events: Option<Vec<Vec<KeySet>>>,
}

/// Extract the dimension-key sets of `checkpoint` for the key spaces that
/// have at least one filtered subscriber, using the same `sui-inverted-index`
/// visitors the persistent index uses.
pub(crate) fn extract_checkpoint_keys(
    checkpoint: &Checkpoint,
    tx_space: bool,
    event_space: bool,
) -> CheckpointKeys {
    CheckpointKeys {
        tx: tx_space.then(|| {
            checkpoint
                .transactions
                .iter()
                .map(|tx| {
                    let mut keys = KeySet::default();
                    // Synthetic universe membership: no write path emits
                    // `TxUniverse`, but exclude-only terms are anchored on it.
                    keys.insert(PrehashedKey::new(encode_dimension_key(
                        IndexDimension::TxUniverse,
                        TX_UNIVERSE_VALUE,
                    )));
                    for_each_transaction_dimension(
                        &tx.transaction,
                        &tx.effects,
                        tx.events.as_ref(),
                        &checkpoint.object_set,
                        |dim, value| {
                            keys.insert(PrehashedKey::new(encode_dimension_key(dim, value)));
                        },
                    );
                    keys
                })
                .collect()
        }),
        events: event_space.then(|| {
            checkpoint
                .transactions
                .iter()
                .map(|tx| {
                    let event_count = tx.events.as_ref().map(|e| e.data.len()).unwrap_or(0);
                    let mut event_keys: Vec<KeySet> = vec![KeySet::default(); event_count];
                    if event_count > 0 {
                        for_each_event_dimension(
                            tx.transaction.sender(),
                            &tx.effects,
                            tx.events.as_ref(),
                            |event_idx, dim, value| {
                                event_keys[event_idx as usize]
                                    .insert(PrehashedKey::new(encode_dimension_key(dim, value)));
                            },
                        );
                    }
                    event_keys
                })
                .collect()
        }),
    }
}

/// One compiled DNF term of one subscriber, registered under its anchor key.
struct TermRef {
    sub: u64,
    term: u32,
}

/// One DNF term with its literal keys encoded and prehashed at registration,
/// so evaluation probes key sets without re-encoding or re-hashing.
struct CompiledTerm {
    includes: Vec<PrehashedKey>,
    excludes: Vec<PrehashedKey>,
}

fn compile_term(term: &BitmapTerm) -> CompiledTerm {
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    for literal in term.literals() {
        match literal {
            BitmapLiteral::Include(key) => {
                includes.push(PrehashedKey::new(key.as_bytes().to_vec()));
            }
            BitmapLiteral::Exclude(key) => {
                excludes.push(PrehashedKey::new(key.as_bytes().to_vec()));
            }
        }
    }
    CompiledTerm { includes, excludes }
}

struct Subscriber {
    spec: SubscriptionSpec,
    sender: mpsc::Sender<SubscriptionUpdate>,
    guard: SubscriptionLifecycleGuard,
    /// Checkpoints processed since this subscriber last received any frame.
    checkpoints_since_frame: u32,
    /// Whether a filtered subscriber still needs its initial progress frame.
    needs_start_frame: bool,
    /// Anchor keys this subscriber registered (one per term), for O(terms)
    /// removal.
    anchor_keys: Vec<PrehashedKey>,
    /// Compiled terms, parallel to the query's; literals prehashed.
    terms: Vec<CompiledTerm>,
}

#[derive(Default)]
pub(crate) struct SubscriptionMatcher {
    subs: HashMap<u64, Subscriber>,
    next_id: u64,
    /// Anchor tables: encoded dimension key -> terms anchored on that key.
    /// Separate tables per key space because Sender/EmitModule/EventType
    /// keys are byte-identical across tx-space and event-space.
    tx_anchor: AnchorTable, // Checkpoints + Transactions subs
    event_anchor: AnchorTable, // Events subs
}

impl SubscriptionMatcher {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.subs.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.subs.is_empty()
    }

    /// Register a subscriber. For a filtered subscription every compiled term
    /// is anchored in the kind-appropriate table under its chosen include key;
    /// unfiltered subscriptions register no anchors and match everything.
    pub(crate) fn insert(
        &mut self,
        spec: SubscriptionSpec,
        sender: mpsc::Sender<SubscriptionUpdate>,
        guard: SubscriptionLifecycleGuard,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let mut anchor_keys = Vec::new();
        let mut terms = Vec::new();
        if let Some(query) = &spec.query {
            let table = match spec.kind {
                SubscriptionKind::Checkpoints | SubscriptionKind::Transactions => {
                    &mut self.tx_anchor
                }
                SubscriptionKind::Events => &mut self.event_anchor,
            };
            anchor_keys.reserve_exact(query.terms().len());
            terms.reserve_exact(query.terms().len());
            for (term_idx, term) in query.terms().iter().enumerate() {
                let key = PrehashedKey::new(select_anchor(term).as_bytes().to_vec());
                table.entry(key.clone()).or_default().push(TermRef {
                    sub: id,
                    term: term_idx as u32,
                });
                anchor_keys.push(key);
                terms.push(compile_term(term));
            }
        }

        let needs_start_frame = spec.query.is_some();

        self.subs.insert(
            id,
            Subscriber {
                spec,
                sender,
                guard,
                checkpoints_since_frame: 0,
                needs_start_frame,
                anchor_keys,
                terms,
            },
        );
        id
    }

    /// Drop a subscriber, purge its term registrations from the anchor tables,
    /// and finalize its lifecycle guard.
    fn remove(&mut self, id: u64, reason: SubscriptionTerminationReason) {
        let Some(sub) = self.subs.remove(&id) else {
            return;
        };
        let table = match sub.spec.kind {
            SubscriptionKind::Checkpoints | SubscriptionKind::Transactions => &mut self.tx_anchor,
            SubscriptionKind::Events => &mut self.event_anchor,
        };
        for key in &sub.anchor_keys {
            if let Some(bucket) = table.get_mut(key) {
                bucket.retain(|term_ref| term_ref.sub != id);
                if bucket.is_empty() {
                    table.remove(key);
                }
            }
        }
        sub.guard.terminate(reason);
    }

    /// Drop every subscriber and finalize all lifecycle guards.
    pub(crate) fn clear(&mut self, reason: SubscriptionTerminationReason) {
        let subscribers = std::mem::take(&mut self.subs);
        self.tx_anchor.clear();
        self.event_anchor.clear();
        for subscriber in subscribers.into_values() {
            subscriber.guard.terminate(reason);
        }
    }

    /// Match `checkpoint` against every subscription and deliver one
    /// [`SubscriptionUpdate`] per subscriber that either matched or is due a
    /// progress frame after `interval` checkpoints without one. Subscribers
    /// whose channel is full or closed are dropped; returns how many departed
    /// this way. `keys` holds the checkpoint's pre-extracted dimension keys
    /// (see [`extract_checkpoint_keys`]); a key space must be present
    /// whenever this matcher holds a filtered subscriber in that space.
    pub(crate) fn dispatch_with_keys(
        &mut self,
        checkpoint: &Arc<Checkpoint>,
        keys: &CheckpointKeys,
        interval: u32,
    ) -> usize {
        if self.is_empty() {
            return 0;
        }

        let cp = checkpoint.summary.sequence_number;
        let tx_hi = checkpoint.summary.data().network_total_transactions;
        let tx_lo = tx_hi - checkpoint.transactions.len() as u64;

        let mut matches: HashMap<u64, SubscriptionMatches> = HashMap::new();

        // Tx-space pass: Checkpoints + Transactions subscriptions.
        if !self.tx_anchor.is_empty() {
            let tx_keys = keys
                .tx
                .as_ref()
                .expect("filtered tx-space subscriber implies extracted tx keys");
            for (i, keys) in tx_keys.iter().enumerate() {
                let i = i as u32;
                for key in keys {
                    let Some(bucket) = self.tx_anchor.get(key) else {
                        continue;
                    };
                    for term_ref in bucket {
                        let sub = &self.subs[&term_ref.sub];
                        // Skip terms once this checkpoint (Checkpoints) or
                        // this transaction (Transactions) already matched for
                        // the subscriber.
                        match matches.get(&term_ref.sub) {
                            Some(SubscriptionMatches::Checkpoint) => continue,
                            Some(SubscriptionMatches::Transactions(ranges))
                                if ranges.last().is_some_and(|r| r.end == i + 1) =>
                            {
                                continue;
                            }
                            _ => {}
                        }
                        if !term_matches(&sub.terms[term_ref.term as usize], keys) {
                            continue;
                        }
                        match sub.spec.kind {
                            SubscriptionKind::Checkpoints => {
                                matches.insert(term_ref.sub, SubscriptionMatches::Checkpoint);
                            }
                            SubscriptionKind::Transactions => {
                                match matches.entry(term_ref.sub).or_insert_with(|| {
                                    SubscriptionMatches::Transactions(Vec::new())
                                }) {
                                    SubscriptionMatches::Transactions(ranges) => {
                                        match ranges.last_mut() {
                                            // Extend the open run when this
                                            // tx is adjacent to it.
                                            Some(r) if r.end == i => r.end = i + 1,
                                            _ => ranges.push(i..i + 1),
                                        }
                                    }
                                    _ => unreachable!("tx-space sub records tx matches"),
                                }
                            }
                            SubscriptionKind::Events => {
                                unreachable!("event subs anchor in event-space")
                            }
                        }
                    }
                }
            }
        }

        // Event-space pass: Events subscriptions.
        if !self.event_anchor.is_empty() {
            let event_keys = keys
                .events
                .as_ref()
                .expect("filtered event subscriber implies extracted event keys");
            for (i, event_keys) in event_keys.iter().enumerate() {
                let i = i as u32;
                for (event_idx, keys) in event_keys.iter().enumerate() {
                    let ev = event_idx as u32;
                    for key in keys {
                        let Some(bucket) = self.event_anchor.get(key) else {
                            continue;
                        };
                        for term_ref in bucket {
                            let sub = &self.subs[&term_ref.sub];
                            // Skip terms once this event already matched for
                            // the subscriber.
                            if let Some(SubscriptionMatches::Events(txs)) =
                                matches.get(&term_ref.sub)
                                && let Some((tx_idx, evs)) = txs.last()
                                && *tx_idx == i
                                && evs.last() == Some(&ev)
                            {
                                continue;
                            }
                            if !term_matches(&sub.terms[term_ref.term as usize], keys) {
                                continue;
                            }
                            match matches
                                .entry(term_ref.sub)
                                .or_insert_with(|| SubscriptionMatches::Events(Vec::new()))
                            {
                                SubscriptionMatches::Events(txs) => match txs.last_mut() {
                                    Some((tx_idx, evs)) if *tx_idx == i => evs.push(ev),
                                    _ => txs.push((i, vec![ev])),
                                },
                                _ => unreachable!("event-space sub records event matches"),
                            }
                        }
                    }
                }
            }
        }

        // Whether any transaction carries events, computed once for the
        // unfiltered-events synthesis below.
        let any_events = checkpoint
            .transactions
            .iter()
            .any(|tx| tx.events.as_ref().is_some_and(|e| !e.data.is_empty()));

        // Delivery loop.
        let mut departed = Vec::new();
        for (&id, sub) in &mut self.subs {
            // `try_send` only notices a departed client when a frame is
            // due, so a filtered subscriber whose filter isn't matching
            // would otherwise keep its slot (and anchor entries) on the
            // not-yet-due tick path for up to `interval` checkpoints after
            // disconnecting. Reap eagerly instead: one atomic load per
            // subscriber per checkpoint.
            if sub.sender.is_closed() {
                departed.push((id, SubscriptionTerminationReason::ClientClosed));
                continue;
            }

            if sub.needs_start_frame {
                sub.needs_start_frame = false;
                if cp > 0 {
                    match sub.sender.try_send(SubscriptionUpdate::WatermarkTick {
                        checkpoint: cp - 1,
                        tx_hi: tx_lo,
                    }) {
                        Ok(()) => {
                            trace!("successfully enqueued start frame for subscriber");
                            sub.checkpoints_since_frame = 0;
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            trace!("unable to enqueue start frame for closed subscriber");
                            departed.push((id, SubscriptionTerminationReason::ClientClosed));
                            continue;
                        }
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            trace!("unable to enqueue start frame for slow subscriber");
                            departed.push((id, SubscriptionTerminationReason::SlowConsumer));
                            continue;
                        }
                    }
                }
            }

            let payload = if sub.spec.query.is_none() {
                // Unfiltered subscriptions match everything: represent "all"
                // as intent (O(1)) instead of materializing per-subscriber
                // index lists. An empty match set yields no frame, keeping
                // empty checkpoints on the watermark-tick path.
                match sub.spec.kind {
                    SubscriptionKind::Checkpoints => Some(SubscriptionMatches::Checkpoint),
                    SubscriptionKind::Transactions => (!checkpoint.transactions.is_empty())
                        .then_some(SubscriptionMatches::AllTransactions),
                    SubscriptionKind::Events => {
                        any_events.then_some(SubscriptionMatches::AllEvents)
                    }
                }
            } else {
                matches.remove(&id)
            };

            let update = match payload {
                Some(matched) => SubscriptionUpdate::Matched(MatchedCheckpoint {
                    checkpoint: Arc::clone(checkpoint),
                    matches: matched,
                }),
                None => {
                    sub.checkpoints_since_frame += 1;
                    if sub.checkpoints_since_frame < interval {
                        continue;
                    }
                    SubscriptionUpdate::WatermarkTick {
                        checkpoint: cp,
                        tx_hi,
                    }
                }
            };

            match sub.sender.try_send(update) {
                Ok(()) => {
                    trace!("successfully enqueued update for subscriber");
                    sub.checkpoints_since_frame = 0;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    trace!("unable to enqueue update for closed subscriber");
                    departed.push((id, SubscriptionTerminationReason::ClientClosed));
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    trace!("unable to enqueue update for slow subscriber");
                    departed.push((id, SubscriptionTerminationReason::SlowConsumer));
                }
            }
        }
        let departed_count = departed.len();
        for (id, reason) in departed {
            self.remove(id, reason);
        }
        departed_count
    }
}

/// Evaluate one compiled DNF term against a key set: every include key
/// present, every exclude key absent.
fn term_matches(term: &CompiledTerm, keys: &KeySet) -> bool {
    term.includes.iter().all(|key| keys.contains(key))
        && term.excludes.iter().all(|key| !keys.contains(key))
}

/// Choose the anchor include for a term: the include literal with the lowest
/// static density rank (sparsest dimension), tie-broken by longest key bytes
/// (most specific), then first. The rank is a heuristic only — any include is
/// a correct anchor because a term can only match when every include key is
/// present.
fn select_anchor(term: &BitmapTerm) -> &BitmapKey {
    term.literals()
        .iter()
        .filter_map(|literal| match literal {
            BitmapLiteral::Include(key) => Some(key),
            BitmapLiteral::Exclude(_) => None,
        })
        .enumerate()
        .min_by_key(|(idx, key)| {
            (
                density_rank(key.as_bytes()),
                std::cmp::Reverse(key.as_bytes().len()),
                *idx,
            )
        })
        .map(|(_, key)| key)
        .expect("every compiled term has at least one include literal")
}

/// Static density rank per dimension, sparsest (best anchor) first. These could
/// evetually be replaced by an adaptive system that figures out dynamically which
/// keys in a subscriptions filter are sparsest.
fn density_rank(key: &[u8]) -> u8 {
    use IndexDimension::*;
    match key.first().copied().and_then(IndexDimension::from_tag_byte) {
        Some(Sender) => 0,
        Some(AffectedAddress) => 1,
        Some(AffectedObject) => 2,
        Some(EventStreamHead) => 3,
        Some(EventType) => 4,
        Some(MoveCall) => 5,
        Some(EmitModule) => 6,
        Some(AnyPackageWrite) => 7,
        Some(EventExtant) => 8,
        Some(TxUniverse) => 9,
        None => u8::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger_history::filter::event_filter_to_query;
    use crate::ledger_history::filter::transaction_filter_to_query;
    use crate::metrics::SubscriptionMetrics;
    use crate::subscription::SubscriberCounts;
    use move_core_types::account_address::AccountAddress;
    use move_core_types::identifier::Identifier;
    use move_core_types::language_storage::StructTag;
    use std::sync::atomic::Ordering;
    use sui_inverted_index::BitmapQuery;
    use sui_rpc::proto::sui::rpc::v2 as proto;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::event::Event;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    /// Interval high enough that no test below triggers a tick incidentally.
    const NO_TICK: u32 = 1000;

    fn metrics() -> SubscriptionMetrics {
        SubscriptionMetrics::new(&prometheus::Registry::new())
    }

    fn addr(idx: u8) -> SuiAddress {
        TestCheckpointBuilder::derive_address(idx)
    }

    fn sender_literal(address: SuiAddress, negated: bool) -> proto::TransactionLiteral {
        let mut sender = proto::SenderFilter::default();
        sender.address = Some(address.to_string());
        let mut literal = proto::TransactionLiteral::default();
        literal.predicate = Some(proto::transaction_literal::Predicate::Sender(sender));
        literal.negated = negated;
        literal
    }

    fn affected_object_literal(object_id: ObjectID) -> proto::TransactionLiteral {
        let mut filter = proto::AffectedObjectFilter::default();
        filter.object_id = Some(object_id.to_string());
        let mut literal = proto::TransactionLiteral::default();
        literal.predicate = Some(proto::transaction_literal::Predicate::AffectedObject(
            filter,
        ));
        literal
    }

    fn tx_query(literals: Vec<proto::TransactionLiteral>) -> BitmapQuery {
        let mut term = proto::TransactionTerm::default();
        term.literals = literals;
        let mut filter = proto::TransactionFilter::default();
        filter.terms = vec![term];
        transaction_filter_to_query(&filter, 16).unwrap()
    }

    fn event_type_literal(type_str: &str, negated: bool) -> proto::EventLiteral {
        let mut filter = proto::EventTypeFilter::default();
        filter.event_type = Some(type_str.to_owned());
        let mut literal = proto::EventLiteral::default();
        literal.predicate = Some(proto::event_literal::Predicate::EventType(filter));
        literal.negated = negated;
        literal
    }

    fn event_query(literals: Vec<proto::EventLiteral>) -> BitmapQuery {
        let mut term = proto::EventTerm::default();
        term.literals = literals;
        let mut filter = proto::EventFilter::default();
        filter.terms = vec![term];
        event_filter_to_query(&filter, 16).unwrap()
    }

    fn subscribe(
        matcher: &mut SubscriptionMatcher,
        kind: SubscriptionKind,
        query: Option<BitmapQuery>,
        metrics: &SubscriptionMetrics,
    ) -> mpsc::Receiver<SubscriptionUpdate> {
        let (sender, receiver) = mpsc::channel(16);
        let filtered = query.is_some();
        let counters = Arc::new(SubscriberCounts::new(1));
        let reservation = counters
            .try_reserve()
            .expect("test subscription requires subscriber capacity");
        let guard = SubscriptionLifecycleGuard::new(kind, filtered, reservation, metrics);
        matcher.insert(SubscriptionSpec { kind, query }, sender, guard);
        receiver
    }

    /// Extract this matcher's own keys, then dispatch: the standalone-driver
    /// convenience these unit tests use. Production extracts once in the
    /// dispatcher and calls `dispatch_with_keys` directly.
    fn dispatch(matcher: &mut SubscriptionMatcher, checkpoint: &Arc<Checkpoint>, interval: u32) {
        let keys = extract_checkpoint_keys(
            checkpoint,
            !matcher.tx_anchor.is_empty(),
            !matcher.event_anchor.is_empty(),
        );
        matcher.dispatch_with_keys(checkpoint, &keys, interval);
    }

    /// One checkpoint with one transaction per sender index, in order.
    fn checkpoint(seq: u64, senders: &[u8]) -> Arc<Checkpoint> {
        let mut builder = TestCheckpointBuilder::new(seq);
        for &sender in senders {
            builder = builder.start_transaction(sender).finish_transaction();
        }
        // `build_checkpoint` adds this checkpoint's tx count on top, so the
        // built summary reports `network_total_transactions = 100 + len`.
        builder = builder.with_network_total_transactions(100);
        Arc::new(builder.build_checkpoint())
    }

    fn recv_matches(receiver: &mut mpsc::Receiver<SubscriptionUpdate>) -> SubscriptionMatches {
        match receiver.try_recv().expect("expected a delivered update") {
            SubscriptionUpdate::Matched(matched) => matched.matches,
            SubscriptionUpdate::WatermarkTick { .. } => panic!("expected a match, got a tick"),
        }
    }

    fn recv_tick(receiver: &mut mpsc::Receiver<SubscriptionUpdate>) -> (u64, u64) {
        match receiver.try_recv().expect("expected a delivered update") {
            SubscriptionUpdate::WatermarkTick { checkpoint, tx_hi } => (checkpoint, tx_hi),
            SubscriptionUpdate::Matched(_) => panic!("expected a tick, got a match"),
        }
    }

    fn recv_start_tick(receiver: &mut mpsc::Receiver<SubscriptionUpdate>, checkpoint: &Checkpoint) {
        let cp = checkpoint.summary.sequence_number;
        assert!(cp > 0, "a start tick requires a pre-checkpoint position");
        let tx_lo = checkpoint.summary.data().network_total_transactions
            - checkpoint.transactions.len() as u64;
        assert_eq!(recv_tick(receiver), (cp - 1, tx_lo));
    }

    /// Flattened matched transaction indices (the payload itself is
    /// run-length encoded).
    fn transaction_indices(matches: SubscriptionMatches) -> Vec<u32> {
        match matches {
            SubscriptionMatches::Transactions(ranges) => ranges.into_iter().flatten().collect(),
            _ => panic!("expected transaction matches"),
        }
    }

    fn event_indices(matches: SubscriptionMatches) -> Vec<(u32, Vec<u32>)> {
        match matches {
            SubscriptionMatches::Events(txs) => txs,
            _ => panic!("expected event matches"),
        }
    }

    #[test]
    fn sender_filtered_transactions_match_only_that_sender() {
        let mut matcher = SubscriptionMatcher::default();
        let metrics = metrics();
        let query = tx_query(vec![sender_literal(addr(0), false)]);
        let mut receiver = subscribe(
            &mut matcher,
            SubscriptionKind::Transactions,
            Some(query),
            &metrics,
        );

        // Senders 0, 1, 0: only indices 0 and 2 match, after the filtered
        // subscription's initial progress frame.
        let cp1 = checkpoint(1, &[0, 1, 0]);
        dispatch(&mut matcher, &cp1, NO_TICK);
        recv_start_tick(&mut receiver, &cp1);
        assert_eq!(transaction_indices(recv_matches(&mut receiver)), vec![0, 2]);

        // A checkpoint with only sender 1 produces no frame.
        dispatch(&mut matcher, &checkpoint(2, &[1, 1]), NO_TICK);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn adjacent_transaction_matches_coalesce_into_ranges() {
        let mut matcher = SubscriptionMatcher::default();
        let metrics = metrics();
        let query = tx_query(vec![sender_literal(addr(0), false)]);
        let mut receiver = subscribe(
            &mut matcher,
            SubscriptionKind::Transactions,
            Some(query),
            &metrics,
        );

        // Senders 0, 0, 1, 0: adjacent matches 0-1 coalesce into one run,
        // the non-adjacent match 3 opens a new one.
        let cp1 = checkpoint(1, &[0, 0, 1, 0]);
        dispatch(&mut matcher, &cp1, NO_TICK);
        recv_start_tick(&mut receiver, &cp1);
        match recv_matches(&mut receiver) {
            SubscriptionMatches::Transactions(ranges) => {
                assert_eq!(ranges, vec![0..2, 3..4]);
            }
            _ => panic!("expected transaction matches"),
        }
    }

    #[test]
    fn checkpoint_filter_matches_once_and_ticks_on_interval() {
        let mut matcher = SubscriptionMatcher::default();
        let metrics = metrics();
        let query = tx_query(vec![sender_literal(addr(0), false)]);
        let mut receiver = subscribe(
            &mut matcher,
            SubscriptionKind::Checkpoints,
            Some(query),
            &metrics,
        );

        // Two matching txs still yield exactly one checkpoint match, preceded
        // by the filtered subscription's initial progress frame.
        let cp1 = checkpoint(1, &[0, 0]);
        dispatch(&mut matcher, &cp1, 2);
        recv_start_tick(&mut receiver, &cp1);
        assert!(matches!(
            recv_matches(&mut receiver),
            SubscriptionMatches::Checkpoint
        ));
        assert!(receiver.try_recv().is_err());

        // Non-matching checkpoints tick only once the interval elapses.
        dispatch(&mut matcher, &checkpoint(2, &[1]), 2);
        assert!(receiver.try_recv().is_err());
        dispatch(&mut matcher, &checkpoint(3, &[1]), 2);
        assert_eq!(recv_tick(&mut receiver), (3, 101));

        // A match resets the counter...
        dispatch(&mut matcher, &checkpoint(4, &[1]), 2);
        assert!(receiver.try_recv().is_err());
        dispatch(&mut matcher, &checkpoint(5, &[0]), 2);
        assert!(matches!(
            recv_matches(&mut receiver),
            SubscriptionMatches::Checkpoint
        ));
        // ...so the next tick again takes a full interval.
        dispatch(&mut matcher, &checkpoint(6, &[1]), 2);
        assert!(receiver.try_recv().is_err());
        dispatch(&mut matcher, &checkpoint(7, &[1]), 2);
        assert_eq!(recv_tick(&mut receiver), (7, 101));
    }

    #[test]
    fn exclude_only_term_matches_via_synthetic_tx_universe() {
        let mut matcher = SubscriptionMatcher::default();
        let metrics = metrics();
        // NOT sender=0: compiled with a synthetic TxUniverse include, which
        // dispatch must insert into every transaction's key set.
        let query = tx_query(vec![sender_literal(addr(0), true)]);
        let mut receiver = subscribe(
            &mut matcher,
            SubscriptionKind::Transactions,
            Some(query),
            &metrics,
        );

        let checkpoint = checkpoint(1, &[0, 1]);
        dispatch(&mut matcher, &checkpoint, NO_TICK);
        recv_start_tick(&mut receiver, &checkpoint);
        assert_eq!(transaction_indices(recv_matches(&mut receiver)), vec![1]);
    }

    #[test]
    fn event_filters_match_per_event() {
        let package = AccountAddress::random();
        let package_str = ObjectID::from(package).to_canonical_string(true);
        let event = |name: &str| Event {
            package_id: ObjectID::from(package),
            transaction_module: Identifier::new("emitter").unwrap(),
            sender: addr(0),
            type_: StructTag {
                address: package,
                module: Identifier::new("mod_t").unwrap(),
                name: Identifier::new(name).unwrap(),
                type_params: vec![],
            },
            contents: vec![],
        };
        let type_a = format!("{package_str}::mod_t::EventA");

        let mut builder = TestCheckpointBuilder::new(1);
        builder = builder
            .start_transaction(0)
            .with_events(vec![event("EventA"), event("EventB")])
            .finish_transaction();
        builder = builder
            .start_transaction(1)
            .with_events(vec![event("EventA")])
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());

        let mut matcher = SubscriptionMatcher::default();
        let metrics = metrics();
        // Include filter: only EventA events, in both transactions.
        let include = event_query(vec![event_type_literal(&type_a, false)]);
        let mut include_rx = subscribe(
            &mut matcher,
            SubscriptionKind::Events,
            Some(include),
            &metrics,
        );
        // Exclude-only filter: anchored on the natural EventExtant marker.
        let exclude = event_query(vec![event_type_literal(&type_a, true)]);
        let mut exclude_rx = subscribe(
            &mut matcher,
            SubscriptionKind::Events,
            Some(exclude),
            &metrics,
        );

        dispatch(&mut matcher, &checkpoint, NO_TICK);
        recv_start_tick(&mut include_rx, &checkpoint);
        recv_start_tick(&mut exclude_rx, &checkpoint);

        assert_eq!(
            event_indices(recv_matches(&mut include_rx)),
            vec![(0, vec![0]), (1, vec![0])]
        );
        assert_eq!(
            event_indices(recv_matches(&mut exclude_rx)),
            vec![(0, vec![1])]
        );
    }

    #[test]
    fn filtered_subscriber_starts_with_progress_tick_before_match() {
        let mut matcher = SubscriptionMatcher::default();
        let metrics = metrics();
        let query = tx_query(vec![sender_literal(addr(0), false)]);
        let mut receiver = subscribe(
            &mut matcher,
            SubscriptionKind::Transactions,
            Some(query),
            &metrics,
        );
        let checkpoint = checkpoint(7, &[0]);

        dispatch(&mut matcher, &checkpoint, NO_TICK);

        recv_start_tick(&mut receiver, &checkpoint);
        assert_eq!(transaction_indices(recv_matches(&mut receiver)), vec![0]);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn unfiltered_subscriber_starts_with_match_without_tick() {
        let mut matcher = SubscriptionMatcher::default();
        let metrics = metrics();
        let mut receiver = subscribe(&mut matcher, SubscriptionKind::Transactions, None, &metrics);
        let checkpoint = checkpoint(7, &[0]);

        dispatch(&mut matcher, &checkpoint, NO_TICK);

        assert!(matches!(
            recv_matches(&mut receiver),
            SubscriptionMatches::AllTransactions
        ));
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn departed_subscriber_is_purged_from_anchor_tables() {
        let mut matcher = SubscriptionMatcher::default();
        let metrics = metrics();
        let query = tx_query(vec![sender_literal(addr(0), false)]);
        let receiver = subscribe(
            &mut matcher,
            SubscriptionKind::Transactions,
            Some(query),
            &metrics,
        );
        drop(receiver);

        // Delivery fails, so the subscriber and its anchors are removed.
        dispatch(&mut matcher, &checkpoint(1, &[0]), NO_TICK);
        assert!(matcher.is_empty());
        assert!(matcher.tx_anchor.is_empty());
        assert_eq!(
            metrics
                .inflight_subscribers
                .with_label_values(&["transaction", "true"])
                .get(),
            0
        );

        // A later matching checkpoint dispatches cleanly with no subscriber.
        dispatch(&mut matcher, &checkpoint(2, &[0]), NO_TICK);
    }

    #[test]
    fn disconnected_subscriber_is_reaped_without_a_due_frame() {
        let mut matcher = SubscriptionMatcher::default();
        let metrics = metrics();
        let query = tx_query(vec![sender_literal(addr(0), false)]);
        let receiver = subscribe(
            &mut matcher,
            SubscriptionKind::Transactions,
            Some(query),
            &metrics,
        );
        drop(receiver);

        // A non-matching checkpoint with the tick not yet due never reaches
        // `try_send`, so only the eager closed-channel check can notice the
        // departed client. It must be reaped now, not `interval` checkpoints
        // later.
        dispatch(&mut matcher, &checkpoint(1, &[1]), NO_TICK);
        assert!(matcher.is_empty());
        assert!(matcher.tx_anchor.is_empty());
        assert_eq!(
            metrics
                .inflight_subscribers
                .with_label_values(&["transaction", "true"])
                .get(),
            0
        );
    }

    #[test]
    fn full_channel_records_one_slow_consumer_termination() {
        let mut matcher = SubscriptionMatcher::default();
        let metrics = metrics();
        let counters = Arc::new(SubscriberCounts::new(1));
        let reservation = counters
            .try_reserve()
            .expect("test subscription requires subscriber capacity");
        let query = tx_query(vec![sender_literal(addr(0), false)]);
        let (sender, _receiver) = mpsc::channel(1);
        let guard = SubscriptionLifecycleGuard::new(
            SubscriptionKind::Transactions,
            true,
            reservation,
            &metrics,
        );
        matcher.insert(
            SubscriptionSpec {
                kind: SubscriptionKind::Transactions,
                query: Some(query),
            },
            sender,
            guard,
        );
        assert_eq!(counters.reserved(), 1);

        // The initial progress frame fills the channel, so the matched
        // payload in the same dispatch classifies the subscriber as slow.
        dispatch(&mut matcher, &checkpoint(1, &[0]), NO_TICK);
        assert!(matcher.is_empty());
        assert_eq!(
            metrics
                .terminations_total
                .with_label_values(&["transaction", "slow_consumer"])
                .get(),
            1
        );
        assert_eq!(
            metrics
                .inflight_subscribers
                .with_label_values(&["transaction", "true"])
                .get(),
            0
        );
        assert_eq!(counters.total.load(Ordering::Relaxed), 0);
        assert_eq!(counters.reserved(), 0);

        drop(matcher);
        assert_eq!(
            metrics
                .terminations_total
                .with_label_values(&["transaction", "slow_consumer"])
                .get(),
            1
        );
    }

    #[test]
    fn select_anchor_prefers_sparse_dimensions_and_skips_excludes() {
        // Sender + AffectedObject includes: the sender key anchors -- a
        // specific account is ~0-1 txs per checkpoint, while a specific
        // object can be a sustained hot spot (e.g. a pool every trade
        // writes).
        let query = tx_query(vec![
            sender_literal(addr(0), false),
            affected_object_literal(ObjectID::random()),
        ]);
        let anchor = select_anchor(&query.terms()[0]);
        assert_eq!(
            IndexDimension::from_tag_byte(anchor.as_bytes()[0]),
            Some(IndexDimension::Sender)
        );

        // Exclude-only term: the anchor is the synthetic TxUniverse include,
        // never the excluded literal.
        let query = tx_query(vec![sender_literal(addr(0), true)]);
        let anchor = select_anchor(&query.terms()[0]);
        assert_eq!(
            IndexDimension::from_tag_byte(anchor.as_bytes()[0]),
            Some(IndexDimension::TxUniverse)
        );
    }
}
