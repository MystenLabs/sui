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
use rand::Rng as _;
use rand::SeedableRng as _;
use rand::rngs::StdRng;
use sui_types::base_types::{AuthorityName, ObjectID};
use sui_types::committee::{Committee, CommitteeTrait as _, EpochId};
use sui_types::crypto::DefaultHash;
use sui_types::digests::TransactionDigest;
use sui_types::error::{SuiErrorKind, SuiResult};
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::transaction::{Transaction, TransactionDataAPI as _, paid_proposer_count};
use tracing::info;

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;

/// Default delay between consecutive slots beyond the free slots. Must exceed the
/// typical (p50) commit latency (~350ms): a firing step's copy then commits before the
/// next step fires, letting drop-on-commit cancel the later copies. A shorter step
/// would have consecutive steps fire before the previous copy commits, manufacturing
/// duplicate copies out of benignly-fanned traffic — turning staggering itself into an
/// amplifier, e.g. after a false-positive activation.
const DEFAULT_STAGGER_STEP: Duration = Duration::from_millis(400);
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

/// The activation signal evaluates a single trailing window of
/// `SIGNAL_WINDOW_COMMITS` commits (~20s at the typical ~15 commits/s, so a mode
/// switch is always backed by at least 20 seconds of data): excess duplicate copies
/// as a fraction of the unique user transactions in the window (the ratio can
/// exceed 1.0 when duplication dominates). Any transition out of band 0 additionally
/// requires the absolute count to reach `SIGNAL_MIN_EXCESS_COPIES` — two excess
/// copies per window commit on average. That materiality floor keeps immaterial
/// duplication on a quiet network (where a tiny denominator makes the ratio noisy)
/// from flipping the mode network-wide: a full-committee fan-out of a single
/// transaction stays under it, while any sustained fan-out crosses it within one
/// window. Identical on every validator (compiled in), so the mode moves in lockstep.
///
/// De-escalation additionally waits out `SIGNAL_EXIT_DWELL_COMMITS` of the ratio
/// sitting below the active band's exit threshold. Staggering suppresses the very
/// duplication the window measures: once active, the held copies drop on commit and
/// the ratio collapses within one window regardless of whether the attack continues,
/// so no exit threshold above zero can hold the band on its own. The dwell is the
/// time-domain hysteresis that the threshold gap cannot provide; it bounds how often
/// the signal relaxes and lets a burst through to re-measure, at the cost of
/// staggering lingering for up to the dwell after an attack stops.
const SIGNAL_WINDOW_COMMITS: usize = 300;
const SIGNAL_MIN_EXCESS_COPIES: u64 = 2 * SIGNAL_WINDOW_COMMITS as u64;
const SIGNAL_EXIT_DWELL_COMMITS: usize = 2 * SIGNAL_WINDOW_COMMITS;

/// One level of the signal's proportional response.
struct SignalBand {
    /// The signal escalates into this band when the window's duplication ratio
    /// reaches this value (a fraction: 0.05 = 5%).
    enter_ratio: f64,
    /// The signal leaves this band when the ratio drops below this value. Kept below
    /// `enter_ratio` so the mode does not flicker at the boundary.
    exit_ratio: f64,
    /// The maximum submission delay (H) staggering uses while this band is active.
    max_delay: Duration,
}

/// The response is proportional to the measured attack: the signal escalates to the
/// highest band whose `enter_ratio` the window meets (leaving band 0 also requires
/// the `SIGNAL_MIN_EXCESS_COPIES` floor), and, once the ratio has stayed below the
/// active band's `exit_ratio` for `SIGNAL_EXIT_DWELL_COMMITS`, de-escalates to the
/// highest band whose `exit_ratio` the ratio still meets (off if none).
///
/// Only `max_delay` (H) varies across bands; the step stays fixed. Under the wrapped
/// schedule H alone sets both sides of the trade-off: how long held traffic waits
/// (≈ H/2 on average) and how many validators share one firing time
/// (≈ committee × step/H), which bounds what a targeted fan-out can amplify.
/// Cheaply tripping the signal therefore buys an attacker only short holds, while
/// the full 4.8s cap engages only once ~20% of committed traffic is duplicate
/// copies — and exploiting the first band's weaker bound lands more copies in
/// commits, escalating the signal by itself. Each cap is a whole multiple of the
/// step, so the last firing step lands exactly on the cap.
const SIGNAL_BANDS: [SignalBand; 2] = [
    SignalBand {
        enter_ratio: 0.05,
        exit_ratio: 0.03,
        max_delay: Duration::from_millis(1600),
    },
    SignalBand {
        enter_ratio: 0.20,
        exit_ratio: 0.15,
        max_delay: Duration::from_millis(4800),
    },
];

/// Human-readable label of a signal band level, for logs and metric labels.
pub fn signal_band_label(band: usize) -> &'static str {
    match band {
        0 => "off",
        1 => "band1",
        2 => "band2",
        _ => "unknown",
    }
}

/// Parameters of the staggering schedule.
#[derive(Debug, Clone)]
pub struct StaggerParams {
    /// Delay between consecutive slots beyond the free slots. Also bounds the
    /// per-submission jitter added on top of the slot's nominal delay.
    pub step: Duration,
    /// Upper bound on the nominal (pre-jitter) delay: slots wrap around the
    /// `max_delay/step` firing steps (see `compute_delay`), so a submitter never
    /// waits longer than this plus half a step of jitter regardless of committee size.
    /// Band-driven: applied signal transitions set it to the active band's hold cap;
    /// it defaults to the first band's, which is also what a manual `set_active(true)`
    /// runs with.
    pub max_delay: Duration,
}

impl Default for StaggerParams {
    fn default() -> Self {
        Self {
            step: DEFAULT_STAGGER_STEP,
            max_delay: SIGNAL_BANDS[0].max_delay,
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
/// that transitions remain observable (logged and counted) even when enablement (the
/// protocol flag together with the node-config kill switch) keeps them from flipping
/// staggering — a dry run ahead of enablement.
struct SignalState {
    /// Per-commit `(excess duplicate copies, unique user transactions)` counts, newest
    /// last, trimmed to `SIGNAL_WINDOW_COMMITS`.
    window: VecDeque<(u64, u64)>,
    /// The signal's band level: 0 is off, `b > 0` is `SIGNAL_BANDS[b - 1]`.
    band: usize,
    /// Consecutive commits the ratio has spent below the active band's exit threshold;
    /// de-escalation fires once it reaches `SIGNAL_EXIT_DWELL_COMMITS`. Reset by any
    /// commit at or above the threshold and by every transition, so each band step
    /// waits out its own dwell.
    below_exit_commits: usize,
}

impl StaggeredSubmission {
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            params: RwLock::new(StaggerParams::default()),
            signal: Mutex::new(SignalState {
                window: VecDeque::new(),
                band: 0,
                below_exit_commits: 0,
            }),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// The duplication signal's band level (0 = off), which moves in dry run too.
    pub fn signal_band(&self) -> usize {
        self.signal.lock().band
    }

    /// Manually flips staggering on or off, independent of the signal (tests and
    /// operator override). Runs with the current schedule params — by default the
    /// first band's hold cap — and is left in place until the signal's next applied
    /// transition overrides it.
    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Relaxed);
    }

    /// Feeds one commit's duplication counts into the activation signal:
    /// `excess_copies` duplicate copies of transactions without allowed proposers
    /// beyond their allowance, against `unique_user_txns` unique user transactions
    /// sequenced. The band state machine always runs on the signal's own state, so
    /// transitions stay observable regardless of enablement; only when `apply` is set
    /// does a transition also flip staggering itself (and retune its hold cap to the
    /// band's). Returns the band entered on a transition (`None` otherwise), together
    /// with the window's duplication ratio — excess copies as a percentage of unique
    /// user transactions, the value the band thresholds were compared against (zero
    /// while the window holds no user transactions; can exceed 100 when duplication
    /// dominates).
    ///
    /// Activation suppresses the very duplication it measures, so under a sustained
    /// attack the signal still cycles, with a mostly-elevated duty cycle: once the
    /// escalating evidence slides out of the window, the ratio sits below the active
    /// band's exit threshold, the dwell runs out and the signal drops to the band the
    /// collapsed ratio supports (usually off), and the brief burst of duplication that
    /// gets through re-escalates it within a few commits. The dwell sets the period of
    /// that cycle (a few windows rather than one).
    pub fn record_commit(
        &self,
        excess_copies: u64,
        unique_user_txns: u64,
        apply: bool,
    ) -> (Option<usize>, f64) {
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
        // An empty window reads as a zero ratio.
        let duplication_ratio = if total == 0 {
            0.0
        } else {
            excess as f64 / total as f64
        };

        // The highest band whose enter ratio the window meets (0 if none).
        let target_band = SIGNAL_BANDS
            .iter()
            .take_while(|band| duplication_ratio >= band.enter_ratio)
            .count();
        let current_band = signal.band;
        // Leaving band 0 additionally requires the materiality floor.
        let escalate =
            target_band > current_band && (current_band > 0 || excess >= SIGNAL_MIN_EXCESS_COPIES);
        // De-escalate once the ratio has stayed below the active band's exit threshold
        // for the dwell, landing on the highest band whose exit threshold the ratio
        // still meets (band 0 if none): the enter/exit gap absorbs boundary noise and
        // the dwell absorbs the ratio collapse that activation itself causes. Dropping
        // to the supported band rather than one step keeps a collapsed ratio from
        // parking the signal in a weaker band for a second dwell.
        let below_exit =
            current_band > 0 && duplication_ratio < SIGNAL_BANDS[current_band - 1].exit_ratio;
        signal.below_exit_commits = if below_exit {
            signal.below_exit_commits + 1
        } else {
            0
        };
        let de_escalate = signal.below_exit_commits >= SIGNAL_EXIT_DWELL_COMMITS;
        let new_band = if escalate {
            target_band
        } else if de_escalate {
            SIGNAL_BANDS
                .iter()
                .take_while(|band| duplication_ratio >= band.exit_ratio)
                .count()
                .min(current_band - 1)
        } else {
            current_band
        };

        debug_assert!(
            !(escalate && de_escalate),
            "escalate and de_escalate cannot be true at the same time"
        );

        let transition = (new_band != current_band).then_some(new_band);
        if let Some(band) = transition {
            signal.band = band;
            signal.below_exit_commits = 0;
            if apply {
                if band > 0 {
                    // Retune before (re)activating so no submission computes a delay
                    // against the previous band's cap.
                    self.params.write().max_delay = SIGNAL_BANDS[band - 1].max_delay;
                }
                self.set_active(band > 0);
            }
            info!(
                "Duplication signal moved to {} (from {}): window duplication ratio \
                 {duplication_ratio:.4} ({excess} excess copies over {total} unique user \
                 transactions in the window){}",
                signal_band_label(band),
                signal_band_label(current_band),
                if apply {
                    ""
                } else {
                    " — staggering not adjusted, disabled by protocol flag or node config"
                },
            );
        }
        (transition, duplication_ratio)
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

        let params = self.params.read();
        let jitter = sample_jitter(params.step);
        compute_delay(&params, slot, proposer_slots, free_slots, jitter)
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

/// Uniform jitter in `[0, step/2)`, sampled fresh per submission from local entropy.
/// The slot schedule is deterministic by design (an observer can derive every
/// validator's slot), so the exact submission instant is blurred locally; bounded well
/// below one step, the jitter never reorders adjacent slots.
fn sample_jitter(step: Duration) -> Duration {
    step.mul_f64(rand::thread_rng().r#gen::<f64>() / 2.0)
}

/// The decision for `slot`, given the `free_slots` immediate slots the gas price pays for
/// and the `proposer_slots` of the transaction's proposer set. The first held slot waits
/// one step; later ones wrap around the `max_delay / step` firing steps rather than
/// saturating at `max_delay`, so no stretch of the (public) order fires simultaneously at
/// the cap, where a fan-out aimed at it would amplify by its whole size with no earlier
/// copy to dedup it.
///
/// `jitter` is added on top of the wrapped nominal delay (never subtracted, so the
/// nominal hold is a floor); free slots stay immediate and get none.
fn compute_delay(
    params: &StaggerParams,
    slot: u64,
    proposer_slots: u64,
    free_slots: u64,
    jitter: Duration,
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
    StaggerDecision::Hold(
        params
            .step
            .saturating_mul(steps)
            .min(params.max_delay)
            .saturating_add(jitter),
    )
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
    fn schedule_delay(
        params: &StaggerParams,
        slot: u64,
        free_slots: u64,
        jitter: Duration,
    ) -> Option<Duration> {
        match compute_delay(params, slot, u64::MAX, free_slots, jitter) {
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
        let no_jitter = Duration::ZERO;
        // Five proposer slots: three free, two held, everything past them rejected.
        assert_eq!(
            compute_delay(&params, 2, 5, 3, no_jitter),
            StaggerDecision::Free
        );
        assert_eq!(
            compute_delay(&params, 4, 5, 3, no_jitter),
            StaggerDecision::Hold(Duration::from_millis(500))
        );
        assert_eq!(
            compute_delay(&params, 5, 5, 3, no_jitter),
            StaggerDecision::Reject
        );
        assert_eq!(
            compute_delay(&params, 200, 5, 3, no_jitter),
            StaggerDecision::Reject
        );
        // Paid amplification widening the free slots past the cut still proposes.
        assert_eq!(
            compute_delay(&params, 6, 5, 7, no_jitter),
            StaggerDecision::Free
        );
        assert_eq!(
            compute_delay(&params, 7, 5, 7, no_jitter),
            StaggerDecision::Reject
        );
        // A cut inside the free slots never shrinks them.
        assert_eq!(
            compute_delay(&params, 2, 1, 3, no_jitter),
            StaggerDecision::Free
        );
        assert_eq!(
            compute_delay(&params, 3, 1, 3, no_jitter),
            StaggerDecision::Reject
        );
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
        let no_jitter = Duration::ZERO;
        // Free slots submit immediately; the first held slot waits one step.
        assert_eq!(schedule_delay(&params, 0, 3, no_jitter), None);
        assert_eq!(schedule_delay(&params, 2, 3, no_jitter), None);
        assert_eq!(
            schedule_delay(&params, 3, 3, no_jitter),
            Some(Duration::from_millis(250))
        );
        assert_eq!(
            schedule_delay(&params, 4, 3, no_jitter),
            Some(Duration::from_millis(500))
        );
        // 2s / 250ms = 8 firing steps: the last one fires exactly at max_delay, then the
        // schedule wraps instead of piling further slots onto the cap.
        assert_eq!(
            schedule_delay(&params, 10, 3, no_jitter),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            schedule_delay(&params, 11, 3, no_jitter),
            Some(Duration::from_millis(250))
        );
        assert_eq!(
            schedule_delay(&params, 120, 3, no_jitter),
            Some(Duration::from_millis(1500))
        );
        // Paid amplification widens the free slots.
        assert_eq!(schedule_delay(&params, 4, 5, no_jitter), None);
        assert_eq!(
            schedule_delay(&params, 5, 5, no_jitter),
            Some(Duration::from_millis(250))
        );
        assert_eq!(schedule_delay(&params, 2, 3, no_jitter), None);

        // Jitter is added on top of the wrapped nominal delay (blurring the exact
        // submission instant an observer could otherwise derive) and never applies to
        // free slots.
        let jitter = Duration::from_millis(100);
        assert_eq!(schedule_delay(&params, 2, 3, jitter), None);
        assert_eq!(
            schedule_delay(&params, 3, 3, jitter),
            Some(Duration::from_millis(350))
        );
        assert_eq!(
            schedule_delay(&params, 120, 3, jitter),
            Some(Duration::from_millis(1600))
        );
    }

    #[test]
    fn sampled_jitter_stays_under_half_a_step_and_varies() {
        let step = Duration::from_millis(250);
        let samples: Vec<Duration> = (0..64).map(|_| sample_jitter(step)).collect();
        assert!(samples.iter().all(|jitter| *jitter < step / 2));
        assert!(
            samples.iter().any(|jitter| *jitter != samples[0]),
            "jitter never varied"
        );
    }

    mod signal {
        use super::*;

        // Compiled-in bands over a single SIGNAL_WINDOW_COMMITS window: band 1 at
        // >= 5% excess-copy ratio (exit < 3%), band 2 at >= 20% (exit < 15%), with an
        // absolute floor of two excess copies per window commit gating any transition
        // out of band 0; each enter/exit gap is the per-edge anti-flicker hysteresis.

        #[test]
        fn duplication_ratio_tracks_the_window() {
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(10, 100, true).1, 0.1);
            assert_eq!(staggered.record_commit(0, 100, true).1, 0.05);
            // A full window of quiet commits evicts the spike entirely.
            let mut ratio = f64::NAN;
            for _ in 0..SIGNAL_WINDOW_COMMITS {
                ratio = staggered.record_commit(0, 100, true).1;
            }
            assert_eq!(ratio, 0.0);
        }

        #[test]
        fn burst_escalates_to_the_matching_band() {
            // A moderate burst (7%) enters the first band...
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(700, 10000, true).0, Some(1));
            assert!(staggered.is_active());
            // ...while a heavy burst (50%) escalates straight to the second.
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(1000, 2000, true).0, Some(2));
            assert!(staggered.is_active());
        }

        #[test]
        fn escalates_further_as_the_ratio_climbs() {
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(700, 10000, true).0, Some(1));
            // An attacker exploiting the first band's weaker bound lands more copies
            // in commits, driving the window ratio over the second band's threshold.
            assert_eq!(staggered.record_commit(3000, 1000, true).0, Some(2));
            assert!(staggered.is_active());
        }

        #[test]
        fn applied_transitions_retune_the_max_delay() {
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.params.read().max_delay, SIGNAL_BANDS[0].max_delay);
            assert_eq!(staggered.record_commit(1000, 2000, true).0, Some(2));
            assert_eq!(staggered.params.read().max_delay, SIGNAL_BANDS[1].max_delay);
            // A steady 4% dilutes the spike into band 1's range; de-escalating there
            // (after the dwell) retunes the cap down again.
            let mut transitions = Vec::new();
            for _ in 0..SIGNAL_WINDOW_COMMITS + SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record_commit(4, 100, true).0);
                if !transitions.is_empty() {
                    break;
                }
            }
            assert_eq!(transitions, vec![1]);
            assert!(staggered.is_active());
            assert_eq!(staggered.params.read().max_delay, SIGNAL_BANDS[0].max_delay);
        }

        #[test]
        fn band_table_is_ordered_and_step_aligned() {
            let step = StaggerParams::default().step;
            let mut previous_enter = 0.0;
            for band in &SIGNAL_BANDS {
                assert!(band.exit_ratio < band.enter_ratio);
                // record_commit's take_while assumes enter ratios strictly increase.
                assert!(band.enter_ratio > previous_enter);
                previous_enter = band.enter_ratio;
                // Whole multiples of the step: the last firing step lands exactly on
                // the cap.
                assert_eq!(band.max_delay.as_millis() % step.as_millis(), 0);
            }
            assert!(SIGNAL_BANDS[0].max_delay < SIGNAL_BANDS[1].max_delay);
            assert_eq!(SIGNAL_BANDS[0].max_delay.as_millis() / step.as_millis(), 4);
            assert_eq!(SIGNAL_BANDS[1].max_delay.as_millis() / step.as_millis(), 12);
        }

        #[test]
        fn ratio_below_threshold_does_not_activate() {
            let staggered = StaggeredSubmission::new();
            // 4% per commit: the absolute floor is passed but the ratio never is.
            for _ in 0..30 {
                assert_eq!(staggered.record_commit(40, 1000, true).0, None);
            }
            assert!(!staggered.is_active());
        }

        #[test]
        fn floor_blocks_high_ratio_at_low_volume() {
            let staggered = StaggeredSubmission::new();
            // 20% ratio, but exactly two excess copies per commit: the floor holds
            // any escalation back until a full window's worth has accumulated; the
            // ratio then places the signal directly in band 2.
            for _ in 0..SIGNAL_WINDOW_COMMITS - 1 {
                assert_eq!(staggered.record_commit(2, 10, true).0, None);
                assert!(!staggered.is_active());
            }
            assert_eq!(staggered.record_commit(2, 10, true).0, Some(2));
        }

        #[test]
        fn old_spikes_slide_out_of_window() {
            let staggered = StaggeredSubmission::new();
            // 500 excess copies at 50%: below the floor on its own.
            assert_eq!(staggered.record_commit(500, 1000, true).0, None);
            // A full window of quiet commits pushes the spike out, so an identical
            // second spike cannot combine with it to reach the floor.
            for _ in 0..SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(0, 100, true).0, None);
            }
            assert_eq!(staggered.record_commit(500, 1000, true).0, None);
            assert!(!staggered.is_active());
        }

        #[test]
        fn deescalates_straight_to_off_once_quiet_traffic_dilutes_the_spike() {
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(1000, 2000, true).0, Some(2));
            // Quiet traffic dilutes the escalating spike's window ratio to zero; band 2
            // holds until the ratio has sat below its exit threshold for the dwell and
            // then drops straight to off (no band's exit threshold is met), within one
            // window (eviction of the spike) plus one dwell at the latest. Band 1 is
            // not visited on the way down.
            let mut transitions = Vec::new();
            for _ in 0..=SIGNAL_WINDOW_COMMITS + SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record_commit(0, 100, true).0);
                assert_eq!(staggered.is_active(), transitions.is_empty());
            }
            assert_eq!(transitions, vec![0]);
        }

        #[test]
        fn threshold_gaps_hold_the_band_between_exit_and_enter() {
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(1000, 2000, true).0, Some(2));
            // A steady 4% sits inside band 1's 3%..5% gap: as it dilutes the spike
            // the signal settles into band 1 (after the dwell) and then holds there
            // indefinitely...
            let mut transitions = Vec::new();
            for _ in 0..2 * SIGNAL_WINDOW_COMMITS + SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record_commit(4, 100, true).0);
                assert!(staggered.is_active());
            }
            assert_eq!(transitions, vec![1]);
            // ...and once deactivated by a 2% trickle, 4% does not re-activate.
            let mut transitions = Vec::new();
            for _ in 0..SIGNAL_WINDOW_COMMITS + SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record_commit(2, 100, true).0);
            }
            assert_eq!(transitions, vec![0]);
            for _ in 0..2 * SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(4, 100, true).0, None);
                assert!(!staggered.is_active());
            }
        }

        #[test]
        fn gap_holds_band2_between_exit_and_enter() {
            // A steady 17% sits inside band 2's 15%..20% gap: it holds band 2 from
            // above...
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(1000, 2000, true).0, Some(2));
            for _ in 0..2 * SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(17, 100, true).0, None);
            }
            // ...and does not enter it from band 1 below.
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(700, 10000, true).0, Some(1));
            for _ in 0..2 * SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(17, 100, true).0, None);
            }
        }

        #[test]
        fn dry_run_tracks_transitions_without_flipping_staggering() {
            let staggered = StaggeredSubmission::new();
            // Transitions are reported even when not applied...
            assert_eq!(staggered.record_commit(1000, 2000, false).0, Some(2));
            // ...but staggering itself stays untouched, including its hold cap.
            assert!(!staggered.is_active());
            assert_eq!(staggered.params.read().max_delay, SIGNAL_BANDS[0].max_delay);
            // Quiet traffic eventually reports the de-escalations too, still without
            // touching staggering.
            let mut transitions = Vec::new();
            for _ in 0..=SIGNAL_WINDOW_COMMITS + SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record_commit(0, 100, false).0);
                assert!(!staggered.is_active());
            }
            assert_eq!(transitions, vec![0]);
            assert_eq!(staggered.params.read().max_delay, SIGNAL_BANDS[0].max_delay);
        }

        #[test]
        fn exit_dwell_absorbs_transient_dips() {
            let staggered = StaggeredSubmission::new();
            assert_eq!(staggered.record_commit(1000, 2000, true).0, Some(2));
            let exit_ratio = SIGNAL_BANDS[1].exit_ratio;

            // Quiet traffic takes the ratio below band 2's exit threshold, but the
            // band holds until the dwell has elapsed below it...
            let mut below_exit = 0;
            while below_exit < SIGNAL_EXIT_DWELL_COMMITS / 2 {
                let (transition, ratio) = staggered.record_commit(0, 100, true);
                assert_eq!(transition, None);
                if ratio < exit_ratio {
                    below_exit += 1;
                }
            }
            // ...and a single commit heavy enough to lift the diluted window back
            // above the threshold (no further escalation: band 2 is the top) restarts
            // the dwell from scratch.
            let (transition, ratio) = staggered.record_commit(10_000, 100, true);
            assert_eq!(transition, None);
            assert!(ratio >= exit_ratio);
            assert_eq!(staggered.signal_band(), 2);

            let mut below_exit = 0;
            loop {
                let (transition, ratio) = staggered.record_commit(0, 100, true);
                if ratio < exit_ratio {
                    below_exit += 1;
                }
                if below_exit < SIGNAL_EXIT_DWELL_COMMITS {
                    assert_eq!(transition, None, "de-escalated before the dwell elapsed");
                } else {
                    assert_eq!(transition, Some(0), "dwell elapsed without de-escalating");
                    break;
                }
            }
            assert!(!staggered.is_active());
            assert_eq!(staggered.signal_band(), 0);
        }

        #[test]
        fn signal_state_is_independent_of_manual_activation() {
            let staggered = StaggeredSubmission::new();
            staggered.set_active(true);
            // The signal's own state machine starts deactivated, so quiet commits produce
            // no transition and manual activating is left in place.
            for _ in 0..50 {
                assert_eq!(staggered.record_commit(0, 100, true).0, None);
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
        let no_jitter = Duration::ZERO;
        let mut tail_delays = std::collections::HashSet::new();
        for slot in 3..200u64 {
            let delay = schedule_delay(&params, slot, 3, no_jitter).unwrap();
            assert!(delay >= params.step, "slot {slot} waited {delay:?}");
            assert!(delay <= params.max_delay, "slot {slot} waited {delay:?}");
            assert_eq!(
                delay,
                schedule_delay(&params, slot + firing_steps, 3, no_jitter).unwrap(),
                "schedule does not cycle at slot {slot}"
            );
            if slot >= 3 + firing_steps {
                tail_delays.insert(delay);
            }
        }
        assert_eq!(tail_delays.len() as u64, firing_steps);
        // Paid amplification shifts the wrap origin with the free slots.
        assert_eq!(
            schedule_delay(&params, 5 + firing_steps, 5, no_jitter),
            Some(Duration::from_millis(250))
        );
        // Degenerate params (one firing step) still respect the max_delay bound.
        let degenerate = StaggerParams {
            step: Duration::from_secs(3),
            max_delay: Duration::from_secs(2),
        };
        for slot in 1..10u64 {
            assert_eq!(
                schedule_delay(&degenerate, slot, 1, no_jitter),
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

        // Past the bounded delay plus half a step of jitter the entry is proposed as usual.
        tokio::time::sleep(Duration::from_millis(900)).await;
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

        // Past the bounded delay plus half a step of jitter the bundle is proposed atomically.
        tokio::time::sleep(Duration::from_millis(900)).await;
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
