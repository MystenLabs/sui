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
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{AuthorityName, ObjectID};
use sui_types::committee::{Committee, CommitteeTrait as _, EpochId};
use sui_types::crypto::DefaultHash;
use sui_types::digests::TransactionDigest;
use sui_types::error::{SuiErrorKind, SuiResult};
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::transaction::{Transaction, TransactionDataAPI as _, paid_proposer_count};
use tracing::info;

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;

/// Default delay between consecutive slots beyond the free slots. It exceeds the median
/// commit latency of about 350ms, so one step's copy commits before the next step fires
/// and the later copies are dropped. A shorter step would turn honest fan-out into
/// duplicate copies.
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
/// "unrestricted" when the user transactions could have named their proposers and
/// did not (the class staggering targets), "restricted" when they did, and "na" for
/// groups without user transactions (system messages). Only the first user
/// transaction is inspected: admission requires soft bundle members to name the same
/// allowed proposers, so it speaks for the group.
pub fn proposers_metric_label(
    transactions: &[ConsensusTransaction],
    epoch_store: &AuthorityPerEpochStore,
) -> &'static str {
    let Some(transaction) = transactions
        .iter()
        .find_map(|transaction| transaction.kind.as_user_transaction())
    else {
        return "na";
    };
    if transaction
        .data()
        .transaction_data()
        .expiration()
        .restricts_proposers(epoch_store.epoch())
    {
        "restricted"
    } else {
        "unrestricted"
    }
}

const DEFAULT_SIGNAL_WINDOW_COMMITS: usize = 300;
const DEFAULT_SIGNAL_MIN_EXCESS_COPIES_PER_COMMIT: u64 = 2;
/// Twenty windows, about 7 minutes on mainnet. Mainnet duplication is continuous, so
/// each de-escalation lets a burst of copies through before the signal escalates again.
/// A long dwell makes this rare. Its cost is that staggering stays on for the dwell
/// after an attack stops.
const DEFAULT_SIGNAL_EXIT_DWELL_COMMITS: usize = 20 * DEFAULT_SIGNAL_WINDOW_COMMITS;

/// Number of signal bands above "off".
const NUM_SIGNAL_BANDS: usize = 2;

/// Labels of the band levels, indexed by level. Level 0 is "off".
const SIGNAL_BAND_LABELS: [&str; NUM_SIGNAL_BANDS + 1] = ["off", "band1", "band2"];

/// One level of the signal's response. See [`SignalConfig`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalBand {
    /// The signal enters this band when the ratio reaches this value.
    pub enter_ratio: f64,
    /// The signal may leave this band when the ratio falls below this value.
    pub exit_ratio: f64,
    /// The hold cap that staggering uses while this band is active.
    pub max_delay: Duration,
}

/// Honest duplication from driver backups and retries is about 1.5% of mainnet traffic,
/// well under band 1's 10%. Mainnet's measured unpaid duplication is 2 to 16 times the
/// unique traffic, which clears band 2's 50% within seconds.
///
/// Only the hold cap varies across bands. A held transaction waits about half the cap
/// on average. About `committee * step / max_delay` validators share each firing time,
/// which bounds how much a fan-out aimed at one firing time can amplify. A cheap attack
/// that only trips band 1 therefore causes only short holds. Each cap is a whole
/// multiple of the step. The caps are compiled in, and the ratios come from the protocol
/// config.
const DEFAULT_SIGNAL_BANDS: [SignalBand; NUM_SIGNAL_BANDS] = [
    SignalBand {
        enter_ratio: 0.10,
        exit_ratio: 0.05,
        max_delay: Duration::from_millis(1600),
    },
    SignalBand {
        enter_ratio: 0.50,
        exit_ratio: 0.25,
        max_delay: Duration::from_millis(4800),
    },
];

/// Thresholds of the duplication signal, which turns staggering on and off.
///
/// The signal looks at the last `window_commits` commits. Its ratio is the number of
/// excess duplicate copies divided by the number of unique user transactions in that
/// window. The ratio exceeds 1.0 when copies outnumber unique transactions.
///
/// The signal has a level: 0 is off, and level `b > 0` is `bands[b - 1]`. It escalates
/// to the highest band whose enter ratio the window meets. Leaving level 0 also
/// requires at least `min_excess_copies` in the window. On a quiet network the ratio
/// has a small denominator, and this floor keeps that noise from turning staggering on.
///
/// Staggering suppresses the duplication that the window measures. The ratio therefore
/// drops soon after activation even when the attack continues, and an exit ratio alone
/// cannot keep the band active. The signal de-escalates only after the ratio stays
/// below the active band's exit ratio for `exit_dwell_commits` consecutive commits.
/// This wait is the exit dwell. The signal then drops to the highest band whose exit
/// ratio the ratio still meets, or to off.
///
/// The inputs come from the commit history and the protocol config, so all validators
/// compute the same level. A restarted validator starts at level 0. It catches up
/// within about one window plus one dwell, unless the ratio sits between a band's exit
/// and enter ratios.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalConfig {
    pub window_commits: usize,
    pub min_excess_copies: u64,
    pub exit_dwell_commits: usize,
    /// The bands, weakest first. `assert_valid` checks the order.
    pub bands: [SignalBand; NUM_SIGNAL_BANDS],
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            window_commits: DEFAULT_SIGNAL_WINDOW_COMMITS,
            min_excess_copies: DEFAULT_SIGNAL_MIN_EXCESS_COPIES_PER_COMMIT
                * DEFAULT_SIGNAL_WINDOW_COMMITS as u64,
            exit_dwell_commits: DEFAULT_SIGNAL_EXIT_DWELL_COMMITS,
            bands: DEFAULT_SIGNAL_BANDS,
        }
    }
}

impl SignalConfig {
    /// Reads the thresholds from the protocol config. A protocol version without a value
    /// uses the default for that field.
    pub fn from_protocol_config(config: &ProtocolConfig) -> Self {
        let defaults = Self::default();
        let window_commits = config
            .staggered_submission_signal_window_commits_as_option()
            .map_or(defaults.window_commits, |commits| commits as usize)
            .max(1);
        let min_excess_copies = config
            .staggered_submission_signal_min_excess_copies_per_commit_as_option()
            .map_or(defaults.min_excess_copies, |per_commit| {
                per_commit * window_commits as u64
            });
        let exit_dwell_commits = config
            .staggered_submission_signal_exit_dwell_commits_as_option()
            .map_or(defaults.exit_dwell_commits, |commits| commits as usize);
        let ratio =
            |bps: Option<u64>, default: f64| bps.map_or(default, |bps| bps as f64 / 10_000.0);
        let bands = [
            SignalBand {
                enter_ratio: ratio(
                    config.staggered_submission_signal_band1_enter_ratio_bps_as_option(),
                    defaults.bands[0].enter_ratio,
                ),
                exit_ratio: ratio(
                    config.staggered_submission_signal_band1_exit_ratio_bps_as_option(),
                    defaults.bands[0].exit_ratio,
                ),
                max_delay: defaults.bands[0].max_delay,
            },
            SignalBand {
                enter_ratio: ratio(
                    config.staggered_submission_signal_band2_enter_ratio_bps_as_option(),
                    defaults.bands[1].enter_ratio,
                ),
                exit_ratio: ratio(
                    config.staggered_submission_signal_band2_exit_ratio_bps_as_option(),
                    defaults.bands[1].exit_ratio,
                ),
                max_delay: defaults.bands[1].max_delay,
            },
        ];
        let signal_config = Self {
            window_commits,
            min_excess_copies,
            exit_dwell_commits,
            bands,
        };
        signal_config.assert_valid();
        signal_config
    }

    /// Panics unless the bands are ordered weakest first. `record_commit` relies on this
    /// order to find the target band. Protocol config values are fixed per version, so a
    /// bad table fails on every validator and in tests.
    fn assert_valid(&self) {
        // De-escalation from band 0 would underflow with a zero dwell.
        assert!(
            self.exit_dwell_commits > 0,
            "signal exit dwell must be at least one commit: {self:?}"
        );
        for (i, band) in self.bands.iter().enumerate() {
            assert!(
                band.exit_ratio > 0.0,
                "signal band {} exit ratio must be positive: {band:?}",
                i + 1
            );
            assert!(
                band.exit_ratio < band.enter_ratio,
                "signal band {} exit ratio must be below its enter ratio: {band:?}",
                i + 1
            );
        }
        for (i, pair) in self.bands.windows(2).enumerate() {
            let (lower, higher) = (&pair[0], &pair[1]);
            assert!(
                higher.enter_ratio > lower.enter_ratio,
                "signal band {} enter ratio must exceed band {}'s: {self:?}",
                i + 2,
                i + 1
            );
            assert!(
                higher.max_delay >= lower.max_delay,
                "signal band {} max delay must not be below band {}'s: {self:?}",
                i + 2,
                i + 1
            );
        }
    }
}

/// The effect of one commit on the duplication signal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CommitSignal {
    /// The level the signal moved to, if it moved.
    pub transition: Option<usize>,
    /// The level before this commit.
    pub previous_band: usize,
    /// Commits spent at `previous_band`, including this one.
    pub commits_in_previous_band: u64,
    pub duplication_ratio: f64,
}

impl CommitSignal {
    #[cfg(test)]
    fn pair(&self) -> (Option<usize>, f64) {
        (self.transition, self.duplication_ratio)
    }
}

/// Label of a signal level, for logs and metrics.
pub fn signal_band_label(band: usize) -> &'static str {
    SIGNAL_BAND_LABELS.get(band).copied().unwrap_or("unknown")
}

/// Parameters of the staggering schedule.
#[derive(Debug, Clone)]
pub struct StaggerParams {
    /// Delay between consecutive slots beyond the free slots. Jitter adds at most half
    /// of it.
    pub step: Duration,
    /// Upper bound on the delay before jitter: slots wrap around the `max_delay/step`
    /// firing steps (see `compute_delay`), so a submitter never waits longer than this
    /// plus half a step, regardless of committee size. Each signal transition sets it to
    /// the active band's cap. It starts at band 1's cap, which `set_active(true)` uses.
    pub max_delay: Duration,
}

impl Default for StaggerParams {
    fn default() -> Self {
        Self {
            step: DEFAULT_STAGGER_STEP,
            max_delay: DEFAULT_SIGNAL_BANDS[0].max_delay,
        }
    }
}

/// Decides whether this validator should delay submitting a given user transaction to
/// consensus, and by how much. It is inactive by default. The duplication signal in
/// `record_commit` turns it on and off. A validator that restarts mid-epoch rebuilds its
/// signal window from the commits after recovery, so its mode can lag its peers for a
/// while. This is acceptable because staggering is a local policy.
pub struct StaggeredSubmission {
    active: AtomicBool,
    params: RwLock<StaggerParams>,
    config: SignalConfig,
    /// True when both the `staggered_submission_signal` protocol flag and
    /// `NodeConfig::enable_staggered_submission_signal` are set. When false, the signal
    /// still runs and reports transitions but does not change staggering.
    enabled: bool,
    signal: Mutex<SignalState>,
}

/// State of the duplication signal. It is kept apart from `active` so that transitions
/// are logged and counted even when the signal is not enabled.
struct SignalState {
    /// `(excess copies, unique user transactions)` for each commit in the window,
    /// newest last.
    window: VecDeque<(u64, u64)>,
    /// The signal level: 0 is off, and `b > 0` is `SignalConfig::bands[b - 1]`.
    band: usize,
    /// Consecutive commits with the ratio below the active band's exit ratio. Any
    /// commit at or above the exit ratio resets it, and so does every transition.
    below_exit_commits: usize,
    /// Commits since the last transition.
    commits_in_band: u64,
}

impl StaggeredSubmission {
    pub fn new(config: SignalConfig, enabled: bool) -> Self {
        Self {
            active: AtomicBool::new(false),
            params: RwLock::new(StaggerParams::default()),
            config,
            enabled,
            signal: Mutex::new(SignalState {
                window: VecDeque::new(),
                band: 0,
                below_exit_commits: 0,
                commits_in_band: 0,
            }),
        }
    }

    pub fn config(&self) -> &SignalConfig {
        &self.config
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// The signal level. It changes even when the signal is not enabled.
    pub fn signal_band(&self) -> usize {
        self.signal.lock().band
    }

    /// Turns staggering on or off by hand, for tests and operator overrides. The next
    /// transition of an enabled signal overrides this setting.
    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Relaxed);
    }

    /// Adds one commit to the signal window and updates the signal level.
    /// `excess_copies` counts the copies of unrestricted transactions beyond their paid
    /// allowance, and `unique_user_txns` counts the unique user transactions in the
    /// commit. When the signal is enabled, a transition also sets staggering on or off
    /// and sets the hold cap to the new band's.
    ///
    /// Under a sustained attack the signal still cycles, because staggering suppresses
    /// the duplication it measures. The ratio falls, the dwell runs out, and the signal
    /// drops, usually to off. The copies that then get through escalate it again within
    /// a few commits. The dwell sets how often this happens.
    pub fn record_commit(&self, excess_copies: u64, unique_user_txns: u64) -> CommitSignal {
        let config = &self.config;
        let mut signal = self.signal.lock();
        signal.commits_in_band += 1;
        signal.window.push_back((excess_copies, unique_user_txns));
        while signal.window.len() > config.window_commits {
            signal.window.pop_front();
        }
        let (excess, total) = signal
            .window
            .iter()
            .fold((0u64, 0u64), |(excess, total), (e, t)| {
                (excess + e, total + t)
            });
        // First copies can leave the window while late copies keep arriving. A window
        // with no unique transactions uses a denominator of one so that the late copies
        // still count.
        let duplication_ratio = excess as f64 / total.max(1) as f64;

        let target_band = config
            .bands
            .iter()
            .take_while(|band| duplication_ratio >= band.enter_ratio)
            .count();
        let current_band = signal.band;
        // Leaving level 0 also requires `min_excess_copies`.
        let escalate =
            target_band > current_band && (current_band > 0 || excess >= config.min_excess_copies);
        // De-escalation drops to the highest band whose exit ratio the ratio still meets,
        // not one band down. Dropping one band at a time would make a collapsed ratio
        // wait out a second dwell in a lower band.
        let below_exit =
            current_band > 0 && duplication_ratio < config.bands[current_band - 1].exit_ratio;
        signal.below_exit_commits = if below_exit {
            signal.below_exit_commits + 1
        } else {
            0
        };
        let de_escalate = signal.below_exit_commits >= config.exit_dwell_commits;
        let new_band = if escalate {
            target_band
        } else if de_escalate {
            config
                .bands
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
        let commits_in_previous_band = signal.commits_in_band;
        if let Some(band) = transition {
            signal.band = band;
            signal.below_exit_commits = 0;
            signal.commits_in_band = 0;
            if self.enabled {
                if band > 0 {
                    // Set the cap before activating so that no submission uses the
                    // previous band's cap.
                    self.params.write().max_delay = config.bands[band - 1].max_delay;
                }
                self.set_active(band > 0);
            }
            info!(
                "Duplication signal moved to {} (from {}): window duplication ratio \
                 {duplication_ratio:.4} ({excess} excess copies over {total} unique user \
                 transactions in the window){}",
                signal_band_label(band),
                signal_band_label(current_band),
                if self.enabled {
                    ""
                } else {
                    " — staggering not adjusted, disabled by protocol flag or node config"
                },
            );
        }
        CommitSignal {
            transition,
            previous_band: current_band,
            commits_in_previous_band,
            duplication_ratio,
        }
    }

    #[cfg(test)]
    pub fn set_params_for_testing(&self, params: StaggerParams) {
        *self.params.write() = params;
    }

    /// Returns `(transition, duplication_ratio)` from `record_commit`.
    #[cfg(test)]
    fn record_commit_pair(
        &self,
        excess_copies: u64,
        unique_user_txns: u64,
    ) -> (Option<usize>, f64) {
        self.record_commit(excess_copies, unique_user_txns).pair()
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
        let own_index = epoch_store
            .own_committee_index()
            .expect("staggering decisions are only made on validators");

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

/// Returns a uniform random jitter in `[0, step/2)`, drawn from local entropy for each
/// submission. Anyone can compute every validator's slot, and the jitter hides the exact
/// submission time. It stays below one step, so it never reorders adjacent slots.
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
/// `jitter` is added to the hold, so the hold never falls below its scheduled delay.
/// Free slots get no jitter.
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

    /// Fixed thresholds for the tests, so that protocol config changes do not change the
    /// test arithmetic.
    const TEST_SIGNAL_WINDOW_COMMITS: usize = 300;
    const TEST_SIGNAL_MIN_EXCESS_COPIES: u64 = 2 * TEST_SIGNAL_WINDOW_COMMITS as u64;
    const TEST_SIGNAL_EXIT_DWELL_COMMITS: usize = 2 * TEST_SIGNAL_WINDOW_COMMITS;
    const TEST_SIGNAL_BANDS: [SignalBand; NUM_SIGNAL_BANDS] = [
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

    fn test_signal_config() -> SignalConfig {
        SignalConfig {
            window_commits: TEST_SIGNAL_WINDOW_COMMITS,
            min_excess_copies: TEST_SIGNAL_MIN_EXCESS_COPIES,
            exit_dwell_commits: TEST_SIGNAL_EXIT_DWELL_COMMITS,
            bands: TEST_SIGNAL_BANDS,
        }
    }

    #[test]
    fn signal_config_is_valid_for_every_protocol_version() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        SignalConfig::default().assert_valid();
        test_signal_config().assert_valid();
        for chain in [Chain::Mainnet, Chain::Testnet, Chain::Unknown] {
            for version in ProtocolVersion::MIN.as_u64()..=ProtocolVersion::MAX.as_u64() {
                let protocol_config =
                    ProtocolConfig::get_for_version(ProtocolVersion::new(version), chain);
                SignalConfig::from_protocol_config(&protocol_config);
            }
        }
    }

    #[test]
    #[should_panic(expected = "enter ratio must exceed")]
    fn signal_config_rejects_misordered_bands() {
        let mut config = test_signal_config();
        config.bands.swap(0, 1);
        config.assert_valid();
    }

    #[test]
    #[should_panic(expected = "exit ratio must be below")]
    fn signal_config_rejects_exit_ratio_above_enter_ratio() {
        let mut config = test_signal_config();
        config.bands[0].exit_ratio = config.bands[0].enter_ratio;
        config.assert_valid();
    }

    #[test]
    #[should_panic(expected = "max delay must not be below")]
    fn signal_config_rejects_decreasing_max_delay() {
        let mut config = test_signal_config();
        config.bands[1].max_delay = config.bands[0].max_delay / 2;
        config.assert_valid();
    }

    #[test]
    #[should_panic(expected = "exit dwell must be at least one commit")]
    fn signal_config_rejects_zero_exit_dwell() {
        let mut config = test_signal_config();
        config.exit_dwell_commits = 0;
        config.assert_valid();
    }

    #[test]
    fn signal_band_labels_cover_every_level() {
        assert_eq!(signal_band_label(0), "off");
        assert_eq!(signal_band_label(NUM_SIGNAL_BANDS), "band2");
        assert_eq!(signal_band_label(NUM_SIGNAL_BANDS + 1), "unknown");
    }

    #[test]
    fn signal_config_reads_protocol_config_with_defaults_for_older_versions() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        let latest = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Mainnet);
        let config = SignalConfig::from_protocol_config(&latest);
        assert_eq!(
            config.window_commits,
            latest.staggered_submission_signal_window_commits() as usize
        );
        assert_eq!(
            config.min_excess_copies,
            latest.staggered_submission_signal_min_excess_copies_per_commit()
                * latest.staggered_submission_signal_window_commits()
        );
        assert_eq!(
            config.exit_dwell_commits,
            latest.staggered_submission_signal_exit_dwell_commits() as usize
        );
        use itertools::Itertools as _;
        for (band, (enter, exit)) in config.bands.iter().zip_eq([
            (
                latest.staggered_submission_signal_band1_enter_ratio_bps(),
                latest.staggered_submission_signal_band1_exit_ratio_bps(),
            ),
            (
                latest.staggered_submission_signal_band2_enter_ratio_bps(),
                latest.staggered_submission_signal_band2_exit_ratio_bps(),
            ),
        ]) {
            assert_eq!(band.enter_ratio, enter as f64 / 10_000.0);
            assert_eq!(band.exit_ratio, exit as f64 / 10_000.0);
            assert!(band.exit_ratio < band.enter_ratio);
        }
        // The hold caps do not come from the protocol config.
        assert_eq!(config.bands[0].max_delay, DEFAULT_SIGNAL_BANDS[0].max_delay);
        assert_eq!(config.bands[1].max_delay, DEFAULT_SIGNAL_BANDS[1].max_delay);

        // A version without the values falls back to the defaults.
        let older = ProtocolConfig::get_for_version(ProtocolVersion::new(139), Chain::Mainnet);
        assert!(
            older
                .staggered_submission_signal_window_commits_as_option()
                .is_none()
        );
        assert_eq!(
            SignalConfig::from_protocol_config(&older),
            SignalConfig::default()
        );
    }

    #[test]
    fn record_commit_reports_the_dwell_of_the_band_left() {
        let staggered = StaggeredSubmission::new(test_signal_config(), true);
        let first = staggered.record_commit(1000, 2000);
        assert_eq!(first.transition, Some(2));
        assert_eq!(first.previous_band, 0);
        assert_eq!(first.commits_in_previous_band, 1);

        // The activating commit counts towards band 0.
        let mut recorded_in_band2 = 0;
        let mut left = None;
        for _ in 0..=TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
            let outcome = staggered.record_commit(0, 100);
            recorded_in_band2 += 1;
            assert_eq!(outcome.previous_band, 2);
            if outcome.transition.is_some() {
                left = Some(outcome);
                break;
            }
        }
        let left = left.expect("signal never left band 2");
        assert_eq!(left.transition, Some(0));
        // The count includes every commit after the activation and the commit that
        // de-escalated.
        assert_eq!(left.commits_in_previous_band, recorded_in_band2);
        assert!(left.commits_in_previous_band > TEST_SIGNAL_EXIT_DWELL_COMMITS as u64);
    }

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

        // Jitter adds to held slots and does not apply to free slots.
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

        // The tests use TEST_SIGNAL_BANDS. Band 1 enters at 5% and exits below 3%.
        // Band 2 enters at 20% and exits below 15%. Leaving off requires two excess
        // copies per window commit.

        #[test]
        fn duplication_ratio_tracks_the_window() {
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            assert_eq!(staggered.record_commit(10, 100).duplication_ratio, 0.1);
            assert_eq!(staggered.record_commit(0, 100).duplication_ratio, 0.05);
            // A full window of quiet commits removes the spike.
            let mut ratio = f64::NAN;
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS {
                ratio = staggered.record_commit(0, 100).duplication_ratio;
            }
            assert_eq!(ratio, 0.0);
        }

        #[tokio::test]
        async fn late_copies_activate_without_new_unique_transactions() {
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            staggered.record_commit(0, 1);
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(0, 0).pair(), (None, 0.0));
            }

            // `min_excess_copies` still applies when the ratio is large.
            assert_eq!(
                staggered
                    .record_commit(TEST_SIGNAL_MIN_EXCESS_COPIES - 1, 0)
                    .pair(),
                (None, (TEST_SIGNAL_MIN_EXCESS_COPIES - 1) as f64),
            );
            assert!(!staggered.is_active());
            assert_eq!(
                staggered.record_commit(1, 0).pair(),
                (Some(2), TEST_SIGNAL_MIN_EXCESS_COPIES as f64),
            );
            assert!(staggered.is_active());
            assert_eq!(
                staggered.params.read().max_delay,
                TEST_SIGNAL_BANDS[1].max_delay
            );
        }

        #[test]
        fn burst_escalates_to_the_matching_band() {
            // A 7% burst enters band 1.
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            assert_eq!(staggered.record_commit(700, 10000).transition, Some(1));
            assert!(staggered.is_active());
            // A 50% burst enters band 2 directly.
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            assert_eq!(staggered.record_commit(1000, 2000).transition, Some(2));
            assert!(staggered.is_active());
        }

        #[test]
        fn escalates_further_as_the_ratio_climbs() {
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            assert_eq!(staggered.record_commit(700, 10000).transition, Some(1));
            // More copies in band 1 push the ratio over band 2's enter ratio.
            assert_eq!(staggered.record_commit(3000, 1000).transition, Some(2));
            assert!(staggered.is_active());
        }

        #[test]
        fn applied_transitions_retune_the_max_delay() {
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            assert_eq!(
                staggered.params.read().max_delay,
                TEST_SIGNAL_BANDS[0].max_delay
            );
            assert_eq!(staggered.record_commit(1000, 2000).transition, Some(2));
            assert_eq!(
                staggered.params.read().max_delay,
                TEST_SIGNAL_BANDS[1].max_delay
            );
            // A steady 4% brings the ratio into band 1's range. After the dwell the
            // signal moves to band 1 and lowers the cap.
            let mut transitions = Vec::new();
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record_commit(4, 100).transition);
                if !transitions.is_empty() {
                    break;
                }
            }
            assert_eq!(transitions, vec![1]);
            assert!(staggered.is_active());
            assert_eq!(
                staggered.params.read().max_delay,
                TEST_SIGNAL_BANDS[0].max_delay
            );
        }

        #[test]
        fn band_table_is_ordered_and_step_aligned() {
            let step = StaggerParams::default().step;
            let mut previous_enter = 0.0;
            for band in &TEST_SIGNAL_BANDS {
                assert!(band.exit_ratio < band.enter_ratio);
                // `record_commit` relies on enter ratios that strictly increase.
                assert!(band.enter_ratio > previous_enter);
                previous_enter = band.enter_ratio;
                // The last firing step lands exactly on the cap.
                assert_eq!(band.max_delay.as_millis() % step.as_millis(), 0);
            }
            assert!(TEST_SIGNAL_BANDS[0].max_delay < TEST_SIGNAL_BANDS[1].max_delay);
            assert_eq!(
                TEST_SIGNAL_BANDS[0].max_delay.as_millis() / step.as_millis(),
                4
            );
            assert_eq!(
                TEST_SIGNAL_BANDS[1].max_delay.as_millis() / step.as_millis(),
                12
            );
        }

        #[test]
        fn ratio_below_threshold_does_not_activate() {
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            // At 4% per commit the excess copies pass `min_excess_copies`, but the
            // ratio stays below band 1.
            for _ in 0..30 {
                assert_eq!(staggered.record_commit(40, 1000).transition, None);
            }
            assert!(!staggered.is_active());
        }

        #[test]
        fn floor_blocks_high_ratio_at_low_volume() {
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            // The ratio is 20%, with two excess copies per commit. The signal waits
            // until a full window reaches `min_excess_copies` and then enters band 2.
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS - 1 {
                assert_eq!(staggered.record_commit(2, 10).transition, None);
                assert!(!staggered.is_active());
            }
            assert_eq!(staggered.record_commit(2, 10).transition, Some(2));
        }

        #[test]
        fn old_spikes_slide_out_of_window() {
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            // 500 excess copies at 50% stay below `min_excess_copies`.
            assert_eq!(staggered.record_commit(500, 1000).transition, None);
            // After a full window of quiet commits, a second spike does not add to
            // the first.
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(0, 100).transition, None);
            }
            assert_eq!(staggered.record_commit(500, 1000).transition, None);
            assert!(!staggered.is_active());
        }

        #[test]
        fn deescalates_straight_to_off_once_quiet_traffic_dilutes_the_spike() {
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            assert_eq!(staggered.record_commit(1000, 2000).transition, Some(2));
            // Quiet traffic brings the ratio to zero. After the dwell the signal drops
            // from band 2 straight to off, without visiting band 1.
            let mut transitions = Vec::new();
            for _ in 0..=TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record_commit(0, 100).transition);
                assert_eq!(staggered.is_active(), transitions.is_empty());
            }
            assert_eq!(transitions, vec![0]);
        }

        #[test]
        fn threshold_gaps_hold_the_band_between_exit_and_enter() {
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            assert_eq!(staggered.record_commit(1000, 2000).transition, Some(2));
            // A steady 4% lies between band 1's exit and enter ratios. The signal
            // moves to band 1 and stays there.
            let mut transitions = Vec::new();
            for _ in 0..2 * TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record_commit(4, 100).transition);
                assert!(staggered.is_active());
            }
            assert_eq!(transitions, vec![1]);
            // After 2% turns the signal off, 4% does not turn it back on.
            let mut transitions = Vec::new();
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record_commit(2, 100).transition);
            }
            assert_eq!(transitions, vec![0]);
            for _ in 0..2 * TEST_SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(4, 100).transition, None);
                assert!(!staggered.is_active());
            }
        }

        #[test]
        fn gap_holds_band2_between_exit_and_enter() {
            // A steady 17% lies between band 2's exit and enter ratios. It keeps
            // band 2 active.
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            assert_eq!(staggered.record_commit(1000, 2000).transition, Some(2));
            for _ in 0..2 * TEST_SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(17, 100).transition, None);
            }
            // It also does not move band 1 up to band 2.
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            assert_eq!(staggered.record_commit(700, 10000).transition, Some(1));
            for _ in 0..2 * TEST_SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record_commit(17, 100).transition, None);
            }
        }

        #[test]
        fn dry_run_tracks_transitions_without_flipping_staggering() {
            let staggered = StaggeredSubmission::new(test_signal_config(), false);
            // A disabled signal reports transitions.
            assert_eq!(staggered.record_commit(1000, 2000).transition, Some(2));
            // Staggering and its hold cap do not change.
            assert!(!staggered.is_active());
            assert_eq!(
                staggered.params.read().max_delay,
                TEST_SIGNAL_BANDS[0].max_delay
            );
            // De-escalation is also reported without changing staggering.
            let mut transitions = Vec::new();
            for _ in 0..=TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record_commit(0, 100).transition);
                assert!(!staggered.is_active());
            }
            assert_eq!(transitions, vec![0]);
            assert_eq!(
                staggered.params.read().max_delay,
                TEST_SIGNAL_BANDS[0].max_delay
            );
        }

        #[test]
        fn exit_dwell_absorbs_transient_dips() {
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            assert_eq!(staggered.record_commit(1000, 2000).transition, Some(2));
            let exit_ratio = TEST_SIGNAL_BANDS[1].exit_ratio;

            // Quiet traffic takes the ratio below band 2's exit ratio. The band stays
            // active for half the dwell.
            let mut below_exit = 0;
            while below_exit < TEST_SIGNAL_EXIT_DWELL_COMMITS / 2 {
                let (transition, ratio) = staggered.record_commit_pair(0, 100);
                assert_eq!(transition, None);
                if ratio < exit_ratio {
                    below_exit += 1;
                }
            }
            // One heavy commit lifts the ratio above the exit ratio and restarts the
            // dwell.
            let (transition, ratio) = staggered.record_commit_pair(10_000, 100);
            assert_eq!(transition, None);
            assert!(ratio >= exit_ratio);
            assert_eq!(staggered.signal_band(), 2);

            let mut below_exit = 0;
            loop {
                let (transition, ratio) = staggered.record_commit_pair(0, 100);
                if ratio < exit_ratio {
                    below_exit += 1;
                }
                if below_exit < TEST_SIGNAL_EXIT_DWELL_COMMITS {
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
            let staggered = StaggeredSubmission::new(test_signal_config(), true);
            staggered.set_active(true);
            // The signal starts at off, so quiet commits cause no transition and the
            // manual setting stays.
            for _ in 0..50 {
                assert_eq!(staggered.record_commit(0, 100).transition, None);
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
            state.get_cache_writer().clone(),
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

        // After the delay plus the maximum jitter, the entry is proposed.
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

        // After the delay plus the maximum jitter, the bundle is proposed atomically.
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
            state.get_cache_writer().clone(),
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
            state.get_cache_writer().clone(),
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
