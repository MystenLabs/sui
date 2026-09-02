// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Staggered consensus submission for transactions that do not restrict their proposers.
//!
//! A transaction without a usable allowed-proposers list can be submitted to every validator
//! at once, amplifying its consensus cost while paying gas once. Rejecting such transactions
//! everywhere would break existing clients, so instead — when duplication is detected on the
//! network — each validator derives a deterministic slot for itself from a stake-weighted
//! permutation of the committee seeded by the transaction, and acts on its own submission
//! according to that slot. The first `paid_proposer_count` validators in the derived order
//! submit immediately (the proposer set an explicit list would be allowed), the validators
//! up to the stake cut (`STAGGER_PROPOSER_STAKE_BPS`) wait long enough that an earlier
//! copy can commit first, and every validator past the cut rejects the submission.
//! Duplication is thereby bounded by construction instead of detected after its bandwidth is already spent.
//!
//! Staggering applies on the consensus transaction pool path only: entries are held inside
//! the pool and skipped by `take()` until eligible. A validator configured to use the
//! push-based consensus adapter path instead submits immediately.
//!
//! The slot is derived from the gas object ids rather than the transaction digest: digest
//! derivation could be ground by re-signing trivial variants (nonce, budget) until each
//! variant's first slot lands on a different validator, while all such variants necessarily
//! share their gas objects. Transactions paying from an address balance have no gas objects
//! and fall back to the digest; the residual grindability there is accepted, since the
//! variants still share the sender's address balance and stay visible to sender-level
//! accounting.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use fastcrypto::hash::HashFunction;
use mysten_common::debug_fatal;
use parking_lot::{Mutex, RwLock};
use prometheus::{Histogram, IntCounter, IntCounterVec, IntGauge};
use rand::SeedableRng as _;
use rand::rngs::StdRng;
use sui_types::base_types::{AuthorityName, ObjectID};
use sui_types::committee::{Committee, CommitteeTrait as _, EpochId};
use sui_types::crypto::DefaultHash;
use sui_types::digests::TransactionDigest;
use sui_types::error::{SuiErrorKind, SuiResult};
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::transaction::{Transaction, TransactionDataAPI as _, paid_proposer_count};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;

/// Default delay between consecutive slots beyond the free slots.
const DEFAULT_STAGGER_STEP: Duration = Duration::from_millis(350);
/// Default upper bound on any submission delay.
const DEFAULT_STAGGER_MAX_DELAY: Duration = Duration::from_secs(5);
/// Held (staggered) submissions may occupy at most `capacity / this` of the owner's
/// pending-transaction capacity; see [`StaggerQuota`].
const STAGGERED_HELD_QUOTA_DIVISOR: usize = 4;
/// Share of total voting power, in basis points, that a transaction's proposer set
/// covers: the leading slots of its stagger order whose cumulative stake first reaches
/// this share may propose it (the free slots immediately, the rest after their hold);
/// every later slot rejects the submission outright. This bounds the copies of one
/// transaction that can ever reach consensus — a hold only delays a copy, and when the
/// validator learns of the commit late (processing lag, a slow chain) every held copy
/// fires, so without a cap the amplification bound under load is the whole committee.
/// Stake, not count: a count-based set could land on validators worth less than f,
/// which a single fault budget could take out entirely. 40 % clears the validity
/// threshold (f + 1, which guarantees an honest live member) with margin for slow
/// validators, while still excluding most of the committee.
const STAGGER_PROPOSER_STAKE_BPS: u64 = 4_000;

/// Metric label splitting submissions by allowed-proposers restriction:
/// "unrestricted" when any user transaction in the group could have named its
/// proposers and did not (the class staggering targets), "restricted" when all of
/// them did, and "na" for groups without user transactions (system messages).
pub fn proposers_metric_label(
    transactions: &[ConsensusTransaction],
    epoch_store: &AuthorityPerEpochStore,
) -> &'static str {
    let epoch = epoch_store.epoch();
    let mut saw_user_transaction = false;
    for transaction in transactions
        .iter()
        .filter_map(|transaction| transaction.kind.as_user_transaction())
    {
        saw_user_transaction = true;
        if !transaction
            .data()
            .transaction_data()
            .expiration()
            .restricts_proposers(epoch)
        {
            return "unrestricted";
        }
    }
    if saw_user_transaction {
        "restricted"
    } else {
        "na"
    }
}

/// Activation signal hysteresis over a single trailing window of
/// `SIGNAL_WINDOW_COMMITS` commits (~20s at the typical ~15 commits/s, so a mode
/// switch is always backed by at least 20 seconds of data). The signal activates when
/// excess duplicate copies amount to at least `SIGNAL_ACTIVATE_DUPLICATE_THRESHOLD`
/// percent of the unique user transactions in the window (the ratio can exceed 100%
/// when duplication dominates) and their absolute count reaches
/// `SIGNAL_MIN_EXCESS_COPIES` — the materiality floor keeps a couple of client
/// double-submits on a quiet network from activating on a noisy ratio. It deactivates when the
/// ratio falls to `SIGNAL_DEACTIVATE_DUPLICATE_THRESHOLD` percent or below; the gap
/// is the hysteresis that keeps the mode from flickering around a single boundary.
/// Identical on every validator (compiled in), so the mode flips in lockstep.
const SIGNAL_WINDOW_COMMITS: usize = 300;
const SIGNAL_ACTIVATE_DUPLICATE_THRESHOLD: u64 = 5;
const SIGNAL_DEACTIVATE_DUPLICATE_THRESHOLD: u64 = 3;
const SIGNAL_MIN_EXCESS_COPIES: u64 = 20;

/// Parameters of the staggering schedule.
#[derive(Debug, Clone)]
pub struct StaggerParams {
    /// Delay between consecutive slots beyond the free slots.
    pub step: Duration,
    /// Upper bound on any submission delay: slots wrap around the `max_delay/step`
    /// firing steps (see `compute_delay`), so a submitter never waits longer than
    /// this regardless of committee size.
    pub max_delay: Duration,
}

impl Default for StaggerParams {
    fn default() -> Self {
        Self {
            step: DEFAULT_STAGGER_STEP,
            max_delay: DEFAULT_STAGGER_MAX_DELAY,
        }
    }
}

/// Decides whether this validator should delay submitting a given user transaction to
/// consensus, and by how much. Inactive by default; activated and deactivated by the
/// commit-derived duplication signal (`record_commit`), which every honest validator
/// computes from identical commit output, so the mode flips in lockstep without
/// coordination. A validator that restarts mid-epoch rebuilds its windows only from the
/// commits it processes after recovery, so its flip can lag peers by up to one window —
/// acceptable for local policy.
pub struct StaggeredSubmission {
    active: AtomicBool,
    params: RwLock<StaggerParams>,
    signal: Mutex<SignalState>,
}

/// The duplication signal's own state machine, tracked independently of `active` so
/// that transitions remain observable (logged and counted) even when the protocol flag
/// keeps them from flipping staggering — a dry run ahead of enablement.
struct SignalState {
    /// Per-commit `(excess duplicate copies, unique user transactions)` counts, newest
    /// last, trimmed to `SIGNAL_WINDOW_COMMITS`.
    window: VecDeque<(u64, u64)>,
    activated: bool,
}

impl StaggeredSubmission {
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            params: RwLock::new(StaggerParams::default()),
            signal: Mutex::new(SignalState {
                window: VecDeque::new(),
                activated: false,
            }),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Relaxed);
    }

    /// Feeds one commit's duplication counts into the activation signal:
    /// `excess_copies` duplicate copies of transactions without allowed proposers
    /// beyond their allowance, against `unique_user_txns` unique user transactions
    /// sequenced. The hysteresis state machine always runs on the signal's own activated
    /// state, so transitions stay observable regardless of enablement; only when
    /// `apply` is set does a transition also flip staggering itself. Returns the new
    /// signal state on a transition, `None` otherwise.
    ///
    /// Activation suppresses the very duplication it measures, so under a sustained attack
    /// the mode oscillates with a mostly-activated duty cycle: once the activating evidence
    /// slides out of the window and the ratio drops through the deactivate threshold,
    /// a brief burst of duplication gets through and re-activates it within a few commits.
    pub fn record_commit(
        &self,
        excess_copies: u64,
        unique_user_txns: u64,
        apply: bool,
    ) -> Option<bool> {
        let mut signal = self.signal.lock();
        signal.window.push_back((excess_copies, unique_user_txns));
        while signal.window.len() > SIGNAL_WINDOW_COMMITS {
            signal.window.pop_front();
        }
        let (excess, total) = signal
            .window
            .iter()
            .fold((0u64, 0u64), |(excess, total), (e, t)| {
                (excess + e, total + t)
            });

        let transition = if !signal.activated {
            (excess >= SIGNAL_MIN_EXCESS_COPIES
                && excess * 100 >= total * SIGNAL_ACTIVATE_DUPLICATE_THRESHOLD)
                .then_some(true)
        } else {
            // Deactivate once quiet traffic dilutes (or eviction removes) the activating
            // evidence past the deactivate threshold; the threshold gap absorbs
            // boundary noise after that.
            (excess * 100 <= total * SIGNAL_DEACTIVATE_DUPLICATE_THRESHOLD).then_some(false)
        };

        if let Some(activated) = transition {
            signal.activated = activated;
            if apply {
                self.set_active(activated);
            }
        }
        transition
    }

    #[cfg(test)]
    pub fn set_params_for_testing(&self, params: StaggerParams) {
        *self.params.write() = params;
    }

    /// What this validator does with a group of user transactions — a single transaction
    /// or a soft bundle — bound for consensus: submit now, after a hold, or not at all.
    /// Soft bundle members name the same allowed proposers (enforced at admission), so a
    /// group is restricted or unrestricted as a whole; only an unrestricted group is
    /// subject to staggering, and every member feeds the slot seed, so a bundle cannot be
    /// used to re-roll a member's schedule.
    pub fn submission_delay(
        &self,
        txs: &[&Transaction],
        epoch_store: &AuthorityPerEpochStore,
    ) -> StaggerDecision {
        if !self.is_active() {
            return StaggerDecision::Free;
        }
        if !epoch_store.protocol_config().allowed_proposers() {
            return StaggerDecision::Free;
        }
        self.projected_decision(txs, epoch_store)
    }

    /// The decision staggering would make for `txs` if it were active, ignoring the
    /// activation state and the `allowed_proposers` flag, so an inactive validator can
    /// measure what the schedule would do with its live inflow before enablement.
    pub fn projected_decision(
        &self,
        txs: &[&Transaction],
        epoch_store: &AuthorityPerEpochStore,
    ) -> StaggerDecision {
        let epoch = epoch_store.epoch();
        let tx_data: Vec<_> = txs
            .iter()
            .map(|tx| (tx.data().transaction_data(), tx.digest()))
            .collect();
        // Vacuously free when empty; conservatively held when admission let a mixed
        // bundle through, since a single unrestricted member is enough to amplify.
        if tx_data
            .iter()
            .all(|(tx_data, _)| tx_data.expiration().restricts_proposers(epoch))
        {
            return StaggerDecision::Free;
        }
        // A non-member has no slot in the order; it also has no consensus to submit to.
        let Some(own_index) = epoch_store.own_committee_index() else {
            return StaggerDecision::Free;
        };

        let gas_payment: Vec<ObjectID> = tx_data
            .iter()
            .flat_map(|(tx_data, _)| tx_data.gas_data().payment.iter().map(|(id, _, _)| *id))
            .collect();
        let digests: Vec<&TransactionDigest> = tx_data.iter().map(|(_, digest)| *digest).collect();
        let seed = stagger_seed(&gas_payment, &digests, epoch);
        let committee = epoch_store.committee();
        let order = stagger_order(&seed, committee);
        let slot = slot_in_order(&order, committee, own_index);
        let proposer_slots = stagger_proposer_slots(&order, committee);

        // SIP-45: the gas price buys that many immediate slots, the same sizing an
        // explicit proposer set is allowed. Soft bundles enforce a uniform gas price at
        // admission; min() is the conservative choice should that ever change.
        let gas_price = tx_data
            .iter()
            .map(|(tx_data, _)| tx_data.gas_data().price)
            .min()
            .expect("txs is non-empty");
        let free_slots = paid_proposer_count(gas_price, epoch_store.reference_gas_price());

        compute_delay(&self.params.read(), slot, proposer_slots, free_slots)
    }
}

/// What staggering decides for one submission at this validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaggerDecision {
    /// Submit immediately: staggering is off, the submission is not subject to it, or
    /// this validator holds one of the transaction's free slots.
    Free,
    /// Hold for the given delay, then submit unless a copy was processed meanwhile.
    Hold(Duration),
    /// This validator is outside the transaction's proposer set: reject the submission
    /// so the copy never occupies the queue, and the client moves on to a proposer.
    Reject,
}

impl StaggerDecision {
    /// Metric label for the decision kind.
    pub fn label(&self) -> &'static str {
        match self {
            StaggerDecision::Free => "free",
            StaggerDecision::Hold(_) => "hold",
            StaggerDecision::Reject => "reject",
        }
    }
}

/// The transaction pool's admission gate for staggered (held) submissions, created with
/// the pool's capacity and metrics so slot requests carry no limits or metrics. The
/// stagger policy itself (mode, schedule) stays on the per-epoch [`StaggeredSubmission`]
/// in the epoch store.
///
/// Held submissions are not drained until their delay elapses, so without the quota
/// (`max_pending_transactions / STAGGERED_HELD_QUOTA_DIVISOR`), sustained
/// no-proposer-list inflow at up to `max_delay` of dwell each could fill the pool's
/// capacity and crowd out restricted traffic. At the quota the caller gets a retriable
/// overload error instead — which also steers the client to retry against another
/// validator, plausibly one of the transaction's free-slot validators.
pub struct StaggerQuota {
    quota: usize,
    held: Arc<AtomicUsize>,
    /// Kept in lockstep with `held` for observability.
    metric: IntGauge,
    /// Submissions rejected because this validator is outside the proposer set.
    rejected_metric: IntCounter,
    /// Dry-run records while staggering is inactive: the decision it would have made
    /// per submission and the hold it would have applied.
    shadow_decisions_metric: IntCounterVec,
    shadow_delay_metric: Histogram,
}

impl StaggerQuota {
    pub fn new(
        max_pending_transactions: usize,
        held_metric: IntGauge,
        rejected_metric: IntCounter,
        shadow_decisions_metric: IntCounterVec,
        shadow_delay_metric: Histogram,
    ) -> Self {
        Self {
            quota: (max_pending_transactions / STAGGERED_HELD_QUOTA_DIVISOR).max(1),
            held: Arc::new(AtomicUsize::new(0)),
            metric: held_metric,
            rejected_metric,
            shadow_decisions_metric,
            shadow_delay_metric,
        }
    }

    /// Like [`StaggeredSubmission::submission_delay`], but also registers the hold
    /// against the quota and returns it as a [`StaggeredSlot`] carrying the
    /// eligibility time. A submission outside the proposer set is rejected before it
    /// touches the quota.
    pub fn submission_slot(
        &self,
        txs: &[&Transaction],
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult<Option<StaggeredSlot>> {
        let staggered = epoch_store.staggered_submission();
        if !staggered.is_active() {
            // Dry run: record what would have been held or rejected, hold nothing.
            let decision = staggered.projected_decision(txs, epoch_store);
            self.shadow_decisions_metric
                .with_label_values(&[decision.label()])
                .inc();
            if let StaggerDecision::Hold(delay) = decision {
                self.shadow_delay_metric.observe(delay.as_secs_f64());
            }
            return Ok(None);
        }
        let delay = match staggered.submission_delay(txs, epoch_store) {
            StaggerDecision::Free => return Ok(None),
            StaggerDecision::Hold(delay) => delay,
            StaggerDecision::Reject => {
                self.rejected_metric.inc();
                return Err(SuiErrorKind::StaggeredSubmissionNotProposer.into());
            }
        };
        if self
            .held
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |held| {
                (held < self.quota).then_some(held + 1)
            })
            .is_err()
        {
            return Err(SuiErrorKind::ValidatorOverloadedRetryAfter {
                retry_after_secs: 1,
            }
            .into());
        }
        self.metric.inc();
        Ok(Some(StaggeredSlot {
            delay,
            eligible_at: Instant::now() + delay,
            held: self.held.clone(),
            metric: self.metric.clone(),
            released: false,
        }))
    }
}

/// A held submission's registration against the stagger quota, carrying when the hold
/// elapses. Armed while the entry occupies the caller's queue: dropping it armed (the
/// entry was discarded — insert failure, already-processed exclusion, eviction, flush)
/// releases the registration automatically. `release` marks consumption into a
/// proposal, and `rearm` re-registers on requeue after a dropped acknowledgement —
/// bypassing the quota, just as requeues bypass pool capacity; any excess is
/// transient.
#[must_use]
pub struct StaggeredSlot {
    delay: Duration,
    eligible_at: Instant,
    held: Arc<AtomicUsize>,
    metric: IntGauge,
    released: bool,
}

impl StaggeredSlot {
    /// The delay this slot was created with.
    pub fn delay(&self) -> Duration {
        self.delay
    }

    /// The instant the hold elapses and the entry may be proposed.
    pub fn eligible_at(&self) -> Instant {
        self.eligible_at
    }

    pub fn release(&mut self) {
        debug_assert!(!self.released, "staggered slot released twice");
        self.released = true;
        self.dec();
    }

    pub fn rearm(&mut self) {
        debug_assert!(self.released, "staggered slot rearmed while armed");
        self.released = false;
        self.held.fetch_add(1, Ordering::Relaxed);
        self.metric.inc();
    }

    fn dec(&self) {
        let previous = self.held.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(previous > 0, "staggered held count underflow");
        self.metric.dec();
    }
}

impl Drop for StaggeredSlot {
    fn drop(&mut self) {
        if !self.released {
            self.dec();
        }
    }
}

/// The decision for `slot`, given the `free_slots` immediate slots the gas price pays for
/// and the `proposer_slots` of the transaction's proposer set. The first held slot waits
/// one step; later ones wrap around the `max_delay / step` firing steps rather than
/// saturating at `max_delay`, so no stretch of the (public) order fires simultaneously at
/// the cap, where a fan-out aimed at it would amplify by its whole size with no earlier
/// copy to dedup it.
fn compute_delay(
    params: &StaggerParams,
    slot: u64,
    proposer_slots: u64,
    free_slots: u64,
) -> StaggerDecision {
    if slot < free_slots {
        return StaggerDecision::Free;
    }
    // Paid amplification can widen the free slots past the stake cut: those slots still
    // propose, so the proposer set never shrinks below the free slots.
    if slot >= proposer_slots.max(free_slots) {
        return StaggerDecision::Reject;
    }
    // Degenerate params (step > max_delay) clamp to a single firing step; the final
    // min() then keeps the ≤ max_delay invariant.
    let firing_steps = (params.max_delay.as_millis() / params.step.as_millis().max(1)).max(1);
    let steps = (slot - free_slots) as u128 % firing_steps + 1;
    if steps > u32::MAX as u128 {
        // Unreachable: the step count is bounded by the wrap.
        debug_fatal!("stagger step count {steps} overflows u32");
    }
    let steps = steps.min(u32::MAX as u128) as u32;
    StaggerDecision::Hold(params.step.saturating_mul(steps).min(params.max_delay))
}

impl Default for StaggeredSubmission {
    fn default() -> Self {
        Self::new()
    }
}

/// Seed of the slot permutation for one logical transaction or soft bundle.
///
/// Gas object ids (and, in the fallback, digests) are sorted before hashing: their order
/// inside a transaction or bundle is signer-controlled, and must not offer another
/// grinding dimension.
fn stagger_seed(
    gas_payment: &[ObjectID],
    tx_digests: &[&TransactionDigest],
    epoch: EpochId,
) -> [u8; 32] {
    let mut hasher = DefaultHash::new();
    hasher.update(b"staggered_submission");
    hasher.update(epoch.to_le_bytes());
    if gas_payment.is_empty() {
        let mut digests: Vec<&TransactionDigest> = tx_digests.to_vec();
        digests.sort();
        for digest in digests {
            hasher.update(digest.inner());
        }
    } else {
        let mut ids: Vec<ObjectID> = gas_payment.to_vec();
        ids.sort();
        for id in ids {
            hasher.update(id);
        }
    }
    hasher.finalize().into()
}

/// This validator's slot in a stake-weighted permutation of the committee derived from
/// `seed`: members are drawn without replacement with probability proportional to voting
/// power, the same primitive as `Committee::shuffle_by_stake_from_tx_digest` and the
/// consensus leader schedule. Weighting by stake makes early-slot honesty track honest
/// *stake* (the BFT assumption) rather than validator count, makes splitting stake across
/// seats buy no extra slot-0 share, and lands the implied submission load on validators in
/// proportion to their stake.
fn stagger_order(seed: &[u8; 32], committee: &Committee) -> Vec<AuthorityName> {
    let mut rng = StdRng::from_seed(*seed);
    committee.shuffle_by_stake_with_rng(None, None, &mut rng)
}

/// This validator's slot in the [`stagger_order`] permutation.
#[cfg(test)]
fn stagger_slot(seed: &[u8; 32], committee: &Committee, own_index: u32) -> u64 {
    slot_in_order(&stagger_order(seed, committee), committee, own_index)
}

fn slot_in_order(order: &[AuthorityName], committee: &Committee, own_index: u32) -> u64 {
    let own_name = committee
        .authority_by_index(own_index)
        .expect("own_index is a committee member");
    order
        .iter()
        .position(|name| name == own_name)
        .expect("every committee member appears in the shuffle") as u64
}

/// Size of a transaction's proposer set: the shortest prefix of its [`stagger_order`]
/// whose cumulative voting power reaches [`STAGGER_PROPOSER_STAKE_BPS`] of the total.
/// Slots at or past this index reject the submission.
fn stagger_proposer_slots(order: &[AuthorityName], committee: &Committee) -> u64 {
    let target = committee.total_votes() * STAGGER_PROPOSER_STAKE_BPS / 10_000;
    let mut stake = 0;
    for (index, name) in order.iter().enumerate() {
        stake += committee.weight(name);
        if stake >= target {
            return index as u64 + 1;
        }
    }
    order.len() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_seed(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn slots(committee: &Committee, seed: &[u8; 32]) -> Vec<u64> {
        (0..committee.num_members() as u32)
            .map(|index| stagger_slot(seed, committee, index))
            .collect()
    }

    #[test]
    fn slot_is_a_permutation() {
        for seed_byte in 0..3u8 {
            let seed = test_seed(seed_byte);
            for committee_size in [1usize, 4, 7, 100] {
                let (committee, _) = Committee::new_simple_test_committee_of_size(committee_size);
                let mut slots = slots(&committee, &seed);
                slots.sort();
                assert_eq!(slots, (0..committee_size as u64).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn slot_is_deterministic_and_seed_sensitive() {
        let (committee, _) = Committee::new_simple_test_committee_of_size(100);
        assert_eq!(
            stagger_slot(&test_seed(1), &committee, 42),
            stagger_slot(&test_seed(1), &committee, 42)
        );
        // Different seeds must not produce the same permutation. Compare the full
        // permutations (a single slot can collide legitimately).
        assert_ne!(
            slots(&committee, &test_seed(1)),
            slots(&committee, &test_seed(2))
        );
    }

    #[test]
    fn seed_ignores_gas_payment_order() {
        let id_a = ObjectID::from_single_byte(1);
        let id_b = ObjectID::from_single_byte(2);
        let digest = TransactionDigest::default();
        assert_eq!(
            stagger_seed(&[id_a, id_b], &[&digest], 5),
            stagger_seed(&[id_b, id_a], &[&digest], 5),
        );
    }

    #[test]
    fn seed_depends_on_gas_objects_not_digest() {
        let id = ObjectID::from_single_byte(1);
        let digest_a = TransactionDigest::random();
        let digest_b = TransactionDigest::random();
        // With gas objects, the digest is irrelevant: trivially re-signed variants of the
        // same logical transaction land on the same schedule.
        assert_eq!(
            stagger_seed(&[id], &[&digest_a], 5),
            stagger_seed(&[id], &[&digest_b], 5),
        );
        // Without gas objects (address-balance gas), the digests are the fallback.
        assert_ne!(
            stagger_seed(&[], &[&digest_a], 5),
            stagger_seed(&[], &[&digest_b], 5),
        );
        // The fallback ignores member order, which is submitter-controlled in a bundle.
        assert_eq!(
            stagger_seed(&[], &[&digest_a, &digest_b], 5),
            stagger_seed(&[], &[&digest_b, &digest_a], 5),
        );
    }

    /// `compute_delay` with an unbounded proposer set, as `Option` for the schedule tests.
    fn schedule_delay(params: &StaggerParams, slot: u64, free_slots: u64) -> Option<Duration> {
        match compute_delay(params, slot, u64::MAX, free_slots) {
            StaggerDecision::Free => None,
            StaggerDecision::Hold(delay) => Some(delay),
            StaggerDecision::Reject => panic!("slot {slot} rejected by an unbounded proposer set"),
        }
    }

    #[test]
    fn proposer_set_rejects_slots_past_the_stake_cut() {
        let params = StaggerParams {
            step: Duration::from_millis(250),
            max_delay: Duration::from_secs(2),
        };
        // Five proposer slots: three free, two held, everything past them rejected.
        assert_eq!(compute_delay(&params, 2, 5, 3), StaggerDecision::Free);
        assert_eq!(
            compute_delay(&params, 4, 5, 3),
            StaggerDecision::Hold(Duration::from_millis(500))
        );
        assert_eq!(compute_delay(&params, 5, 5, 3), StaggerDecision::Reject);
        assert_eq!(compute_delay(&params, 200, 5, 3), StaggerDecision::Reject);
        // Paid amplification widening the free slots past the cut still proposes.
        assert_eq!(compute_delay(&params, 6, 5, 7), StaggerDecision::Free);
        assert_eq!(compute_delay(&params, 7, 5, 7), StaggerDecision::Reject);
        // A cut inside the free slots never shrinks them.
        assert_eq!(compute_delay(&params, 2, 1, 3), StaggerDecision::Free);
        assert_eq!(compute_delay(&params, 3, 1, 3), StaggerDecision::Reject);
    }

    #[test]
    fn proposer_slots_cover_the_target_stake() {
        // Equal stake: 40 % of four members needs two of them.
        let (committee, _) = Committee::new_simple_test_committee_of_size(4);
        let order = stagger_order(&test_seed(1), &committee);
        assert_eq!(stagger_proposer_slots(&order, &committee), 2);
        // Ten equal members: 40 % needs four.
        let (committee, _) = Committee::new_simple_test_committee_of_size(10);
        let order = stagger_order(&test_seed(1), &committee);
        assert_eq!(stagger_proposer_slots(&order, &committee), 4);
        // Uneven stake: the cut follows the stake landing in the order, so the set is
        // one member when a 50 % holder leads it and four when the small ones do.
        let (committee, _) =
            Committee::new_simple_test_committee_with_normalized_voting_power(vec![
                5000, 1250, 1250, 1250, 1250,
            ]);
        let whale = committee
            .members()
            .max_by_key(|(_, stake)| *stake)
            .map(|(name, _)| *name)
            .unwrap();
        let mut seen = std::collections::HashSet::new();
        for seed in 0..64u8 {
            let order = stagger_order(&test_seed(seed), &committee);
            let slots = stagger_proposer_slots(&order, &committee);
            let whale_slot = order.iter().position(|name| *name == whale).unwrap() as u64;
            assert_eq!(
                slots,
                if whale_slot == 0 {
                    1
                } else {
                    (whale_slot + 1).min(4)
                }
            );
            assert!(slots as usize <= order.len());
            seen.insert(slots);
        }
        assert!(seen.len() > 1, "stake cut never varied across seeds");
    }

    #[test]
    fn delay_schedule() {
        let params = StaggerParams {
            step: Duration::from_millis(250),
            max_delay: Duration::from_secs(2),
        };
        // Free slots submit immediately; the first held slot waits one step.
        assert_eq!(schedule_delay(&params, 0, 3), None);
        assert_eq!(schedule_delay(&params, 2, 3), None);
        assert_eq!(
            schedule_delay(&params, 3, 3),
            Some(Duration::from_millis(250))
        );
        assert_eq!(
            schedule_delay(&params, 4, 3),
            Some(Duration::from_millis(500))
        );
        // 2s / 250ms = 8 firing steps: the last one fires exactly at max_delay, then the
        // schedule wraps instead of piling further slots onto the cap.
        assert_eq!(schedule_delay(&params, 10, 3), Some(Duration::from_secs(2)));
        assert_eq!(
            schedule_delay(&params, 11, 3),
            Some(Duration::from_millis(250))
        );
        assert_eq!(
            schedule_delay(&params, 120, 3),
            Some(Duration::from_millis(1500))
        );
        // Paid amplification widens the free slots.
        assert_eq!(schedule_delay(&params, 4, 5), None);
        assert_eq!(
            schedule_delay(&params, 5, 5),
            Some(Duration::from_millis(250))
        );
    }

    mod signal {
        use super::*;

        // Compiled-in thresholds over a single SIGNAL_WINDOW_COMMITS window: activate at
        // >= 5% excess-copy ratio with an absolute floor of 20 excess copies; deactivate
        // at <= 3% — the 5%/3% gap is the anti-flicker hysteresis.

        #[test]
        fn activates_on_burst_over_ratio_and_floor() {
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(100, 200, true), Some(true));
            assert!(staggered.is_active());
        }

        #[test]
        fn ratio_below_threshold_does_not_activate() {
            let staggered = StaggeredSubmission::new();
            // 4% per commit: the absolute floor is passed but the ratio never is.
            for _ in 0..30 {
                assert_eq!(staggered.record_commit(4, 100, true), None);
            }
            assert!(!staggered.is_active());
        }

        #[test]
        fn floor_blocks_high_ratio_at_low_volume() {
            let staggered = StaggeredSubmission::new();
            // 10% ratio, but only one excess copy per commit: the floor holds activating
            // back until 20 of them have accumulated in the enter window.
            for _ in 0..19 {
                assert_eq!(staggered.record_commit(1, 10, true), None);
                assert!(!staggered.is_active());
            }
            assert_eq!(staggered.record_commit(1, 10, true), Some(true));
        }

        #[test]
        fn old_spikes_slide_out_of_window() {
            let staggered = StaggeredSubmission::new();
            // 10 excess copies at 10%: below the floor on its own.
            assert_eq!(staggered.record_commit(10, 100, true), None);
            // A full window of quiet commits pushes the spike out, so an identical
            // second spike cannot combine with it to reach the floor.
            for _ in 0..SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(0, 100, true), None);
            }
            assert_eq!(staggered.record_commit(10, 100, true), None);
            assert!(!staggered.is_active());
        }

        #[test]
        fn deactivates_once_quiet_traffic_dilutes_the_spike() {
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(100, 200, true), Some(true));
            // Quiet traffic dilutes the activating spike's window ratio; the mode holds
            // until the ratio crosses the deactivate threshold, and deactivates within
            // one window at the latest (eviction of the spike).
            let mut quiet_commits = 0;
            loop {
                quiet_commits += 1;
                assert!(
                    quiet_commits <= SIGNAL_WINDOW_COMMITS,
                    "never deactivated within a full window"
                );
                match staggered.record_commit(0, 100, true) {
                    None => assert!(staggered.is_active()),
                    Some(activated) => {
                        assert!(!activated);
                        break;
                    }
                }
            }
            assert!(!staggered.is_active());
        }

        #[test]
        fn threshold_gap_holds_mode_between_deactivate_and_activate() {
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(100, 200, true), Some(true));
            // 4% duplication sits inside the 3%..5% gap: activated stays activated, even long
            // after the activating spike has left the window...
            for _ in 0..2 * SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(4, 100, true), None);
                assert!(staggered.is_active());
            }
            // ...and once deactivated by a 2% trickle, 4% does not re-activate.
            let mut transitions = Vec::new();
            for _ in 0..SIGNAL_WINDOW_COMMITS {
                transitions.extend(staggered.record_commit(2, 100, true));
            }
            assert_eq!(transitions, vec![false]);
            for _ in 0..2 * SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(4, 100, true), None);
                assert!(!staggered.is_active());
            }
        }

        #[test]
        fn dry_run_tracks_transitions_without_flipping_staggering() {
            let staggered = StaggeredSubmission::new();
            // Transitions are reported even when not applied...
            assert_eq!(staggered.record_commit(100, 200, false), Some(true));
            // ...but staggering itself stays untouched.
            assert!(!staggered.is_active());
            // Quiet traffic eventually reports the deactivate transition too, still
            // without touching staggering.
            let mut transitions = Vec::new();
            for _ in 0..SIGNAL_WINDOW_COMMITS {
                transitions.extend(staggered.record_commit(0, 100, false));
                assert!(!staggered.is_active());
            }
            assert_eq!(transitions, vec![false]);
        }

        #[test]
        fn signal_state_is_independent_of_manual_activation() {
            let staggered = StaggeredSubmission::new();
            staggered.set_active(true);
            // The signal's own state machine starts deactivated, so quiet commits produce
            // no transition and manual activating is left in place.
            for _ in 0..50 {
                assert_eq!(staggered.record_commit(0, 100, true), None);
            }
            assert!(staggered.is_active());
        }
    }

    #[test]
    fn delay_wraps_instead_of_capping() {
        let params = StaggerParams {
            step: Duration::from_millis(250),
            max_delay: Duration::from_secs(2),
        };
        let firing_steps = 8u64; // 2s / 250ms
        // Every held slot waits at least one step and at most max_delay, and the
        // schedule cycles with the firing-step period, so tail slots spread across all
        // firing times instead of sharing the cap.
        let mut tail_delays = std::collections::HashSet::new();
        for slot in 3..200u64 {
            let delay = schedule_delay(&params, slot, 3).unwrap();
            assert!(delay >= params.step, "slot {slot} waited {delay:?}");
            assert!(delay <= params.max_delay, "slot {slot} waited {delay:?}");
            assert_eq!(
                delay,
                schedule_delay(&params, slot + firing_steps, 3).unwrap(),
                "schedule does not cycle at slot {slot}"
            );
            if slot >= 3 + firing_steps {
                tail_delays.insert(delay);
            }
        }
        assert_eq!(tail_delays.len() as u64, firing_steps);
        // Paid amplification shifts the wrap origin with the free slots.
        assert_eq!(
            schedule_delay(&params, 5 + firing_steps, 5),
            Some(Duration::from_millis(250))
        );
        // Degenerate params (one firing step) still respect the max_delay bound.
        let degenerate = StaggerParams {
            step: Duration::from_secs(3),
            max_delay: Duration::from_secs(2),
        };
        for slot in 1..10u64 {
            assert_eq!(
                schedule_delay(&degenerate, slot, 1),
                Some(Duration::from_secs(2))
            );
        }
    }

    #[test]
    fn seed_depends_on_epoch() {
        let id = ObjectID::from_single_byte(1);
        let digest = TransactionDigest::default();
        assert_ne!(
            stagger_seed(&[id], &[&digest], 5),
            stagger_seed(&[id], &[&digest], 6),
        );
    }
}

/// Staggering through the pull-based transaction pool: entries are held inside the pool
/// and skipped by `take()` until eligible.
#[cfg(test)]
mod pool_tests {
    use std::num::NonZeroUsize;
    use std::sync::Arc;

    use consensus_config::AuthorityIndex;
    use consensus_core::TransactionPool as _;
    use consensus_types::block::{BlockDigest, BlockRef};
    use sui_protocol_config::ProtocolConfig;
    use sui_types::base_types::{ObjectID, SuiAddress};
    use sui_types::crypto::{AccountKeyPair, deterministic_random_account_key};
    use sui_types::messages_consensus::ConsensusTransaction;
    use sui_types::object::Object;
    use sui_types::transaction::MAX_UNPAID_ALLOWED_PROPOSERS;

    use super::*;
    use crate::admission_queue::AdmissionQueueMetrics;
    use crate::authority::AuthorityState;
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use crate::consensus_adapter::consensus_tests::test_user_transaction;
    use crate::consensus_handler::SequencedConsensusTransaction;
    use crate::consensus_transaction_pool::ConsensusTransactionPool;

    /// Ten equal-stake members: the 40 % stake cut makes a proposer set of four slots,
    /// three of them free at the reference gas price, so exactly slot 3 is held and
    /// every later slot is rejected.
    const COMMITTEE_SIZE: u32 = 10;
    const HELD_SLOT: u64 = MAX_UNPAID_ALLOWED_PROPOSERS;

    /// This validator's slot for a transaction or bundle paying with `gas`.
    fn slot_for_gas(epoch_store: &AuthorityPerEpochStore, gas: &[ObjectID]) -> u64 {
        let seed = stagger_seed(gas, &[], epoch_store.epoch());
        stagger_slot(
            &seed,
            epoch_store.committee(),
            epoch_store.own_committee_index().unwrap(),
        )
    }

    /// A state for one of `COMMITTEE_SIZE` validators, plus gas objects ground so that
    /// its slot for a transaction paying with `staggered_gas` is the held slot, while a
    /// transaction paying with `immediate_gas` lands in a free slot. The remaining
    /// candidate objects (all owned by `sender`) are returned for tests that need to
    /// grind their own picks.
    async fn setup() -> (
        Arc<AuthorityState>,
        SuiAddress,
        AccountKeyPair,
        Object,
        Object,
        Vec<Object>,
    ) {
        let (sender, keypair) = deterministic_random_account_key();
        // Each pick has odds 1/COMMITTEE_SIZE per candidate; 128 make a miss negligible.
        let candidates: Vec<Object> = (0..128)
            .map(|_| Object::with_id_owner_for_testing(ObjectID::random(), sender))
            .collect();

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .committee_size(NonZeroUsize::new(COMMITTEE_SIZE as usize).unwrap())
                .with_objects(candidates.clone())
                .build();
        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .build()
            .await;

        let epoch_store = state.epoch_store_for_testing();
        let slot_of = |object: &Object| slot_for_gas(&epoch_store, &[object.id()]);
        let staggered_gas = candidates
            .iter()
            .find(|o| slot_of(o) == HELD_SLOT)
            .expect("no candidate gas object lands on the held slot")
            .clone();
        let immediate_gas = candidates
            .iter()
            .find(|o| slot_of(o) < HELD_SLOT)
            .expect("no candidate gas object lands in a free slot")
            .clone();

        (
            state,
            sender,
            keypair,
            staggered_gas,
            immediate_gas,
            candidates,
        )
    }

    fn pool_for(state: &Arc<AuthorityState>) -> Arc<ConsensusTransactionPool> {
        Arc::new(ConsensusTransactionPool::new_for_tests(
            state.epoch_store_for_testing().clone(),
            10,
            Arc::new(AdmissionQueueMetrics::new_for_tests()),
        ))
    }

    async fn insert(
        pool: &ConsensusTransactionPool,
        state: &Arc<AuthorityState>,
        sender: sui_types::base_types::SuiAddress,
        keypair: &sui_types::crypto::AccountKeyPair,
        gas: Object,
    ) -> (
        crate::consensus_transaction_pool::PositionReceiver,
        ConsensusTransaction,
    ) {
        let tx = test_user_transaction(state, sender, keypair, gas, vec![]).await;
        let consensus_tx =
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx.into());
        let (receiver, newly_inserted) = pool
            .try_insert(pool.epoch(), 1, vec![consensus_tx.clone()])
            .unwrap();
        assert!(newly_inserted);
        (receiver, consensus_tx)
    }

    #[tokio::test]
    async fn held_quota_rejects_further_staggered_inserts() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, _, immediate_gas, candidates) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        // Capacity 10 gives a held quota of 10 / STAGGERED_HELD_QUOTA_DIVISOR = 2.
        let pool = pool_for(&state);

        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_secs(60),
            max_delay: Duration::from_secs(60),
        });
        staggered.set_active(true);

        let mut held_gas = candidates
            .iter()
            .filter(|object| slot_for_gas(&epoch_store, &[object.id()]) == HELD_SLOT)
            .cloned();

        // Fill the quota with held entries.
        for _ in 0..2 {
            let gas = held_gas.next().expect("not enough held-slot gas objects");
            let (_receiver, _) = insert(&pool, &state, sender, &keypair, gas).await;
        }

        // The next entry that would be held is rejected with a retriable error.
        let gas = held_gas.next().expect("not enough held-slot gas objects");
        let tx = test_user_transaction(&state, sender, &keypair, gas, vec![]).await;
        let consensus_tx =
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx.into());
        let error = pool
            .try_insert(pool.epoch(), 1, vec![consensus_tx])
            .unwrap_err();
        assert!(
            matches!(
                error.as_inner(),
                sui_types::error::SuiErrorKind::ValidatorOverloadedRetryAfter { .. }
            ),
            "unexpected quota rejection error: {error}"
        );

        // An immediate-slot transaction is unaffected by the quota and is proposed.
        let (_receiver, _) = insert(&pool, &state, sender, &keypair, immediate_gas).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1, "immediate slot was held");
        // The dropped ack requeues the entry; close() resolves it before the pool drops.
        drop(ack);
        pool.close();
    }

    #[tokio::test]
    async fn staggered_entry_held_until_eligible() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, _, _) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let pool = pool_for(&state);

        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_millis(250),
            max_delay: Duration::from_millis(500),
        });
        staggered.set_active(true);

        let (_receiver, _) = insert(&pool, &state, sender, &keypair, staggered_gas).await;

        // Held: not proposed, but still queued (occupying pool capacity).
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(transactions.is_empty(), "held entry was proposed");
        drop(ack);
        assert_eq!(pool.queue_depth("user"), 1);

        // Past the (bounded) delay the entry is proposed as usual.
        tokio::time::sleep(Duration::from_millis(600)).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1, "eligible entry was not proposed");
        // The dropped ack requeues the entry; close() resolves it before the pool drops.
        drop(ack);
        pool.close();
    }

    #[tokio::test]
    async fn immediate_slot_and_inactive_not_held() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, immediate_gas, _) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let pool = pool_for(&state);

        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_secs(60),
            max_delay: Duration::from_secs(60),
        });
        staggered.set_active(true);

        // Slot 0 for its gas object: proposed on the next take while staggering is active.
        let (_receiver, _) = insert(&pool, &state, sender, &keypair, immediate_gas).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1, "immediate slot was held");
        ack(BlockRef::new(
            1,
            AuthorityIndex::new_for_test(0),
            BlockDigest::MIN,
        ));

        // Inactive: a slot that would be held is proposed immediately.
        staggered.set_active(false);
        let (_receiver, _) = insert(&pool, &state, sender, &keypair, staggered_gas).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1, "inactive staggering held an entry");
        // The dropped ack requeues the entry; close() resolves it before the pool drops.
        drop(ack);
        pool.close();
    }

    #[tokio::test]
    async fn held_entry_resolved_when_copy_processed() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, _, _) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let pool = pool_for(&state);

        // A hold long enough that the entry can only leave the pool by resolution.
        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_secs(60),
            max_delay: Duration::from_secs(60),
        });
        staggered.set_active(true);

        let (receiver, consensus_tx) = insert(&pool, &state, sender, &keypair, staggered_gas).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(transactions.is_empty(), "held entry was proposed");
        drop(ack);

        // While the entry is held, another validator's copy is processed. The next
        // take resolves it without proposing it or waiting out the hold.
        let key = SequencedConsensusTransaction::new_test(consensus_tx).key();
        epoch_store.process_notifications(std::iter::once(&key));
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(transactions.is_empty(), "processed entry was proposed");
        drop(ack);
        assert_eq!(pool.queue_depth("user"), 0);
        assert!(
            receiver.await.unwrap().is_err(),
            "held entry should resolve as already processed"
        );
    }

    #[tokio::test]
    async fn soft_bundle_with_unrestricted_members_held() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, _, immediate_gas, candidates) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let pool = pool_for(&state);

        let staggered = epoch_store.staggered_submission();
        staggered.set_params_for_testing(StaggerParams {
            step: Duration::from_millis(250),
            max_delay: Duration::from_millis(500),
        });
        staggered.set_active(true);

        // A soft bundle's slot derives from the union of its members' gas objects:
        // grind a companion so the bundle is held even though one member would land
        // first on its own. Soft bundles are external fan-out and must not bypass the hold.
        let companion = candidates
            .iter()
            .filter(|object| object.id() != immediate_gas.id())
            .find(|object| {
                slot_for_gas(&epoch_store, &[immediate_gas.id(), object.id()]) == HELD_SLOT
            })
            .expect("no companion makes the bundle land on the held slot")
            .clone();

        let tx_a = test_user_transaction(&state, sender, &keypair, immediate_gas, vec![]).await;
        let tx_b = test_user_transaction(&state, sender, &keypair, companion, vec![]).await;
        let bundle = vec![
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx_a.into()),
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx_b.into()),
        ];
        let (_receiver, newly_inserted) = pool.try_insert(pool.epoch(), 1, bundle).unwrap();
        assert!(newly_inserted);
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(
            transactions.is_empty(),
            "unrestricted soft bundle was not held"
        );
        drop(ack);

        // Past the (bounded) delay the bundle is proposed atomically.
        tokio::time::sleep(Duration::from_millis(600)).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(
            transactions.len(),
            2,
            "released bundle was not proposed whole"
        );
        // The dropped ack requeues the bundle; close() resolves it before the pool drops.
        drop(ack);
        pool.close();
    }

    #[tokio::test]
    async fn slot_past_the_proposer_set_rejected() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, _, _, candidates) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let pool = Arc::new(ConsensusTransactionPool::new_for_tests(
            epoch_store.clone(),
            10,
            metrics.clone(),
        ));
        epoch_store.staggered_submission().set_active(true);

        // Past the proposer set: rejected at insert, before the quota or the pool are
        // touched, with the error that sends the client on to a proposer.
        let gas = candidates
            .iter()
            .find(|object| slot_for_gas(&epoch_store, &[object.id()]) > HELD_SLOT)
            .expect("no candidate gas object lands past the proposer set")
            .clone();
        let tx = test_user_transaction(&state, sender, &keypair, gas, vec![]).await;
        let consensus_tx =
            ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx.into());
        let error = pool
            .try_insert(pool.epoch(), 1, vec![consensus_tx])
            .unwrap_err();
        assert!(
            matches!(
                error.as_inner(),
                SuiErrorKind::StaggeredSubmissionNotProposer
            ),
            "unexpected rejection error: {error}"
        );
        assert_eq!(metrics.pool_staggered_rejected.get(), 1);
        assert_eq!(metrics.pool_staggered_held.get(), 0);
        assert_eq!(pool.queue_depth("user"), 0);
    }

    #[tokio::test]
    async fn inactive_staggering_records_shadow_decisions() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, staggered_gas, immediate_gas, candidates) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let pool = Arc::new(ConsensusTransactionPool::new_for_tests(
            epoch_store.clone(),
            10,
            metrics.clone(),
        ));
        let shadow = |decision: &str| {
            metrics
                .pool_stagger_shadow_decisions
                .with_label_values(&[decision])
                .get()
        };
        assert!(!epoch_store.staggered_submission().is_active());

        // Inactive: nothing is held or rejected, but each would-be decision is recorded.
        let (_receiver, _) = insert(&pool, &state, sender, &keypair, staggered_gas).await;
        assert_eq!(
            (shadow("free"), shadow("hold"), shadow("reject")),
            (0, 1, 0)
        );
        assert_eq!(metrics.pool_stagger_shadow_delay.get_sample_count(), 1);
        assert_eq!(metrics.pool_staggered_held.get(), 0);

        let (_receiver, _) = insert(&pool, &state, sender, &keypair, immediate_gas).await;
        assert_eq!(
            (shadow("free"), shadow("hold"), shadow("reject")),
            (1, 1, 0)
        );

        let past_proposer_set = candidates
            .iter()
            .find(|object| slot_for_gas(&epoch_store, &[object.id()]) > HELD_SLOT)
            .expect("no candidate gas object lands past the proposer set")
            .clone();
        let (_receiver, _) = insert(&pool, &state, sender, &keypair, past_proposer_set).await;
        assert_eq!(
            (shadow("free"), shadow("hold"), shadow("reject")),
            (1, 1, 1)
        );
        assert_eq!(metrics.pool_staggered_rejected.get(), 0);

        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(
            transactions.len(),
            3,
            "inactive staggering held or rejected an entry"
        );
        drop(ack);
        pool.close();
    }
}
