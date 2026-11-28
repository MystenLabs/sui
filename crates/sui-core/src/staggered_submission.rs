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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
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

use crate::admission_queue::AdmissionQueueMetrics;
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;

/// Default delay between consecutive slots beyond the free slots. It exceeds the median
/// commit latency of about 350ms, so one step's copy commits before the next step fires
/// and the later copies are dropped. A shorter step would turn honest fan-out into
/// duplicate copies.
const DEFAULT_STAGGER_STEP: Duration = Duration::from_millis(400);
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

const DEFAULT_SIGNAL_WINDOW_COMMITS: u64 = 300;
const DEFAULT_SIGNAL_MIN_EXCESS_COPIES_PER_COMMIT: u64 = 2;
/// Twenty windows, about 7 minutes on mainnet. Mainnet duplication is continuous, so
/// each de-escalation lets a burst of copies through before the signal escalates again.
/// A long dwell makes this rare. Its cost is that staggering stays on for the dwell
/// after an attack stops.
const DEFAULT_SIGNAL_EXIT_DWELL_COMMITS: u64 = 20 * DEFAULT_SIGNAL_WINDOW_COMMITS;

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

/// How the signal measures the length of its window and of its exit dwell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SignalClock {
    /// Each commit advances the clock by one, so lengths are counts of commits. Protocol
    /// versions up to 144 use this clock.
    Commits,
    /// Each commit advances the clock by the milliseconds from the previous commit's
    /// consensus timestamp to its own, so lengths are durations of consensus time.
    /// Commit timestamps are the same on every validator, so validators agree on the
    /// distance between any two commits' positions. The clock itself starts at zero when
    /// a validator starts, so its value is not comparable across validators. Commit
    /// timestamps do not decrease, and `record_commit` measures each gap from the largest
    /// timestamp so far in case one does. A gap longer than `max_commit_gap_ms` advances
    /// the clock by `max_commit_gap_ms` only. After a consensus stall the first commit
    /// therefore adds at most `max_commit_gap_ms` to the window's span and to the exit
    /// dwell. `assert_valid` requires the cap to be below the window and the dwell, so
    /// that this commit cannot empty the window or run a whole dwell on its own.
    CommitTimeMs { max_commit_gap_ms: u64 },
}

impl SignalClock {
    /// How far a commit advances the clock, given the milliseconds from the previous
    /// commit's timestamp to its own.
    fn advance(&self, gap_ms: u64) -> u64 {
        match self {
            Self::Commits => 1,
            Self::CommitTimeMs { max_commit_gap_ms } => gap_ms.min(*max_commit_gap_ms),
        }
    }
}

/// Thresholds of the duplication signal, which turns staggering on and off.
///
/// The signal looks at the commits of the last `window` units of `clock`. Its ratio is
/// the number of excess duplicate copies divided by the number of unique user
/// transactions in that window. The ratio exceeds 1.0 when copies outnumber unique
/// transactions.
///
/// The signal has a level: 0 is off, and level `b > 0` is `bands[b - 1]`. It escalates
/// to the highest band whose enter ratio the window meets. Leaving level 0 also
/// requires at least `min_excess_copies` in the window. On a quiet network the ratio
/// has a small denominator, and this floor keeps that noise from turning staggering on.
///
/// A window whose consensus commits hold fewer than `min_consensus_user_txns` user
/// transactions, duplicates included, is a low-volume window. The signal does not
/// escalate on a low-volume window, whatever its ratio, and it counts the window towards
/// the exit dwell. This limits staggering to periods of high consensus load. Both floors
/// are sums over the window, so one large commit passes them only if it carries a full
/// window's worth of transactions. With the time clock the window has a fixed duration,
/// so the floors are fixed rates per second whatever the commit rate is.
///
/// Staggering suppresses the duplication that the window measures. The ratio therefore
/// drops soon after activation even when the attack continues, and an exit ratio alone
/// cannot keep the band active. The signal de-escalates only after `exit_dwell` units of
/// `clock` in which every commit has a ratio below the active band's exit ratio or a
/// low-volume window. This wait is the exit dwell. The signal then drops to the highest
/// band whose exit ratio the ratio still meets, or to off. A dwell that ends on a
/// low-volume window drops it to off.
///
/// The inputs come from the commit history and the protocol config, so all validators
/// compute the same level. A restarted validator starts at level 0. It catches up
/// within about one window plus one dwell, unless the ratio sits between a band's exit
/// and enter ratios.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalConfig {
    pub clock: SignalClock,
    /// The window length, in units of `clock`.
    pub window: u64,
    pub min_excess_copies: u64,
    /// The window must hold this many user transactions, duplicates included. Like
    /// `min_excess_copies`, it is a total over the window. Zero disables the check.
    pub min_consensus_user_txns: u64,
    /// The exit dwell, in units of `clock`.
    pub exit_dwell: u64,
    /// The bands, weakest first. `assert_valid` checks the order.
    pub bands: [SignalBand; NUM_SIGNAL_BANDS],
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            clock: SignalClock::Commits,
            window: DEFAULT_SIGNAL_WINDOW_COMMITS,
            min_excess_copies: DEFAULT_SIGNAL_MIN_EXCESS_COPIES_PER_COMMIT
                * DEFAULT_SIGNAL_WINDOW_COMMITS,
            min_consensus_user_txns: 0,
            exit_dwell: DEFAULT_SIGNAL_EXIT_DWELL_COMMITS,
            bands: DEFAULT_SIGNAL_BANDS,
        }
    }
}

impl SignalConfig {
    /// Reads the thresholds from the protocol config. A version that sets
    /// `staggered_submission_signal_window_ms` uses the time clock and must set the other
    /// time thresholds. Otherwise the signal uses the commit clock. With
    /// `staggered_submission_signal` on, a commit-clock version must set the window, the
    /// excess-copy floor and the exit dwell. Any other missing value uses the default.
    pub fn from_protocol_config(config: &ProtocolConfig) -> Self {
        let defaults = Self::default();
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
        // A version that mixes the two sets would have one set ignored without notice.
        let [
            window_commits,
            min_excess_per_commit,
            min_consensus_per_commit,
            dwell_commits,
        ] = [
            config.staggered_submission_signal_window_commits_as_option(),
            config.staggered_submission_signal_min_excess_copies_per_commit_as_option(),
            config
                .staggered_submission_signal_min_consensus_user_transactions_per_commit_as_option(),
            config.staggered_submission_signal_exit_dwell_commits_as_option(),
        ];
        let commit_fields_set = [
            window_commits,
            min_excess_per_commit,
            min_consensus_per_commit,
            dwell_commits,
        ]
        .iter()
        .any(Option::is_some);
        let time_fields_set = [
            config.staggered_submission_signal_min_excess_copies_per_second_as_option(),
            config
                .staggered_submission_signal_min_consensus_user_transactions_per_second_as_option(),
            config.staggered_submission_signal_exit_dwell_ms_as_option(),
            config.staggered_submission_signal_max_commit_gap_ms_as_option(),
        ]
        .iter()
        .any(Option::is_some);
        let signal_config = match config.staggered_submission_signal_window_ms_as_option() {
            Some(window_ms) => {
                assert!(
                    !commit_fields_set,
                    "the time clock needs the commit-based signal fields unset"
                );
                Self::with_time_clock(config, window_ms, bands)
            }
            None => {
                assert!(
                    !time_fields_set,
                    "the time-based signal fields need staggered_submission_signal_window_ms"
                );
                // A version with the signal on must state its thresholds instead of
                // running the compiled defaults.
                assert!(
                    !config.staggered_submission_signal()
                        || [window_commits, min_excess_per_commit, dwell_commits]
                            .iter()
                            .all(Option::is_some),
                    "the staggered_submission_signal flag needs \
                     staggered_submission_signal_window_ms or the commit-based window, \
                     excess-copy floor and exit dwell"
                );
                Self::with_commit_clock(config, bands)
            }
        };
        signal_config.assert_valid();
        signal_config
    }

    /// Uses the commit clock. The protocol config gives the floors per commit, and the
    /// signal compares the window's totals with these values times the window.
    fn with_commit_clock(config: &ProtocolConfig, bands: [SignalBand; NUM_SIGNAL_BANDS]) -> Self {
        let defaults = Self::default();
        let window = config
            .staggered_submission_signal_window_commits_as_option()
            .unwrap_or(defaults.window);
        let min_excess_copies = config
            .staggered_submission_signal_min_excess_copies_per_commit_as_option()
            .map_or(defaults.min_excess_copies, |per_commit| {
                per_commit.saturating_mul(window)
            });
        let min_consensus_user_txns = config
            .staggered_submission_signal_min_consensus_user_transactions_per_commit_as_option()
            .map_or(defaults.min_consensus_user_txns, |per_commit| {
                per_commit.saturating_mul(window)
            });
        let exit_dwell = config
            .staggered_submission_signal_exit_dwell_commits_as_option()
            .unwrap_or(defaults.exit_dwell);
        Self {
            clock: SignalClock::Commits,
            window,
            min_excess_copies,
            min_consensus_user_txns,
            exit_dwell,
            bands,
        }
    }

    /// Uses the time clock. The protocol config gives the floors per second, and the
    /// signal compares the window's totals with these values times the window duration.
    fn with_time_clock(
        config: &ProtocolConfig,
        window_ms: u64,
        bands: [SignalBand; NUM_SIGNAL_BANDS],
    ) -> Self {
        let per_window = |per_second: u64| per_second.saturating_mul(window_ms) / 1_000;
        let max_commit_gap_ms = config
            .staggered_submission_signal_max_commit_gap_ms_as_option()
            .expect("the time clock needs staggered_submission_signal_max_commit_gap_ms");
        let min_excess_copies = per_window(
            config
                .staggered_submission_signal_min_excess_copies_per_second_as_option()
                .expect(
                    "the time clock needs staggered_submission_signal_min_excess_copies_per_second",
                ),
        );
        let min_consensus_user_txns = per_window(
            config
                .staggered_submission_signal_min_consensus_user_transactions_per_second_as_option()
                .expect(
                    "the time clock needs \
                     staggered_submission_signal_min_consensus_user_transactions_per_second",
                ),
        );
        let exit_dwell = config
            .staggered_submission_signal_exit_dwell_ms_as_option()
            .expect("the time clock needs staggered_submission_signal_exit_dwell_ms");
        Self {
            clock: SignalClock::CommitTimeMs { max_commit_gap_ms },
            window: window_ms,
            min_excess_copies,
            min_consensus_user_txns,
            exit_dwell,
            bands,
        }
    }

    /// Panics unless the bands are ordered weakest first. `record_commit` relies on this
    /// order to find the target band. Protocol config values are fixed per version, so a
    /// bad table fails on every validator and in tests.
    fn assert_valid(&self) {
        // De-escalation from band 0 would underflow with a zero dwell.
        assert!(
            self.exit_dwell > 0,
            "signal exit dwell must be positive: {self:?}"
        );
        assert!(self.window > 0, "signal window must be positive: {self:?}");
        if let SignalClock::CommitTimeMs { max_commit_gap_ms } = self.clock {
            // With a zero cap the clock never advances, so commits leave the window only
            // through its count limit and the dwell never ends.
            assert!(
                max_commit_gap_ms > 0,
                "signal max commit gap must be positive: {self:?}"
            );
            // The cap exists so that one commit cannot empty the window or end the
            // dwell.
            assert!(
                max_commit_gap_ms < self.window && max_commit_gap_ms < self.exit_dwell,
                "signal max commit gap must be below the window and the exit dwell: {self:?}"
            );
        }
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
    /// Milliseconds of consensus time spent at `previous_band`, up to this commit. The
    /// gap cap does not apply, so this measures time even when the signal counts
    /// commits.
    pub ms_in_previous_band: u64,
    pub duplication_ratio: f64,
    /// User transactions in the window, duplicates included, compared against
    /// `min_consensus_user_txns`.
    pub window_consensus_user_txns: u64,
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
    /// the active band's cap, and a transition to off sets it back to band 1's cap. It
    /// starts at band 1's cap, which `set_active(true)` and the inactive shadow
    /// decisions use.
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

/// Transaction counts of one commit, or their sum over the signal window.
#[derive(Debug, Clone, Copy, Default)]
struct WindowCounts {
    excess: u64,
    unique: u64,
    consensus_user_txns: u64,
}

impl WindowCounts {
    fn add(&mut self, other: &Self) {
        self.excess += other.excess;
        self.unique += other.unique;
        self.consensus_user_txns += other.consensus_user_txns;
    }

    /// Removes counts that `add` added earlier, so the fields cannot underflow.
    fn sub(&mut self, other: &Self) {
        self.excess -= other.excess;
        self.unique -= other.unique;
        self.consensus_user_txns -= other.consensus_user_txns;
    }
}

/// State of the duplication signal. It is kept apart from `active` so that transitions
/// are logged and counted even when the signal is not enabled.
struct SignalState {
    /// The clock position and counts of each commit in the window, newest last.
    window: VecDeque<(u64, WindowCounts)>,
    /// The sum of `window`. `record_commit` updates it as commits enter and leave the
    /// window.
    window_sum: WindowCounts,
    /// The signal level: 0 is off, and `b > 0` is `SignalConfig::bands[b - 1]`.
    band: usize,
    /// The position of the signal clock, which the newest commit set.
    clock: u64,
    /// The largest consensus timestamp of the commits recorded so far.
    last_commit_timestamp_ms: Option<u64>,
    /// How far the clock has advanced over consecutive commits with the ratio below the
    /// active band's exit ratio or with a low-volume window. Any other commit resets it,
    /// and so does every transition.
    below_exit: u64,
    /// Commits since the last transition.
    commits_in_band: u64,
    /// Milliseconds of consensus time since the last transition.
    ms_in_band: u64,
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
                window_sum: WindowCounts::default(),
                band: 0,
                clock: 0,
                last_commit_timestamp_ms: None,
                below_exit: 0,
                commits_in_band: 0,
                ms_in_band: 0,
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
    /// `commit_timestamp_ms` is the commit's consensus timestamp, which the time clock
    /// and the band duration read. `excess_copies` counts the copies of unrestricted
    /// transactions beyond their paid allowance, `unique_user_txns` counts the unique
    /// user transactions in the commit, and `consensus_user_txns` counts every user
    /// transaction in the commit, unique transactions and duplicates of any kind. When
    /// the signal is enabled, a transition also sets staggering on or off and sets the
    /// hold cap to the new band's.
    ///
    /// Under a sustained attack the signal still cycles, because staggering suppresses
    /// the duplication it measures. The ratio falls, the dwell runs out, and the signal
    /// drops, usually to off. The copies that then get through escalate it again. The
    /// dwell sets how often this happens. The volume floor behaves the same way:
    /// staggering keeps duplicates out of commits, so a peak made of duplicates turns
    /// into a low-volume window once staggering is on.
    ///
    /// Without `min_consensus_user_txns` the signal escalates again within a few
    /// commits. With it, the window total must first climb back over
    /// `min_consensus_user_txns`, which takes a share of a window that depends on how
    /// far the attack exceeds the floor. The copies in those commits are not staggered.
    pub fn record_commit(
        &self,
        commit_timestamp_ms: u64,
        excess_copies: u64,
        unique_user_txns: u64,
        consensus_user_txns: u64,
    ) -> CommitSignal {
        let config = &self.config;
        let mut guard = self.signal.lock();
        let signal = &mut *guard;
        signal.commits_in_band += 1;
        // The first commit of a signal has no gap.
        let gap_ms = signal
            .last_commit_timestamp_ms
            .map_or(0, |previous| commit_timestamp_ms.saturating_sub(previous));
        let advance = config.clock.advance(gap_ms);
        signal.clock += advance;
        signal.ms_in_band += gap_ms;
        // Commit timestamps do not decrease. Keeping the maximum means that a timestamp
        // that did decrease could not count the same time twice.
        signal.last_commit_timestamp_ms = signal
            .last_commit_timestamp_ms
            .max(Some(commit_timestamp_ms));
        let commit = WindowCounts {
            excess: excess_copies,
            unique: unique_user_txns,
            consensus_user_txns,
        };
        signal.window.push_back((signal.clock, commit));
        signal.window_sum.add(&commit);
        // `window > 0`, so the newest commit always stays. Commits with equal timestamps
        // do not advance the time clock, so the window also keeps at most `window`
        // commits. With the commit clock the two limits are the same. With the time
        // clock the count limit is not a tuned threshold: it only bounds the window's
        // memory while commit timestamps do not change, and it binds only at more than
        // one commit per millisecond. `position` is
        // at most `clock`, so the subtraction cannot underflow.
        while let Some(&(position, oldest)) = signal.window.front()
            && (signal.clock - position >= config.window
                || signal.window.len() as u64 > config.window)
        {
            signal.window.pop_front();
            signal.window_sum.sub(&oldest);
        }
        let WindowCounts {
            excess,
            unique: total,
            consensus_user_txns: window_consensus_user_txns,
        } = signal.window_sum;
        let low_volume = window_consensus_user_txns < config.min_consensus_user_txns;
        // First copies can leave the window while late copies keep arriving. A window
        // with no unique transactions uses a denominator of one so that the late copies
        // still count.
        let duplication_ratio = excess as f64 / total.max(1) as f64;
        // A low-volume window counts as a ratio of zero for every band decision. Every
        // exit ratio is positive, so such a window does not escalate, counts towards the
        // exit dwell, and drops the signal to off when the dwell ends.
        let band_ratio = if low_volume { 0.0 } else { duplication_ratio };

        let target_band = config
            .bands
            .iter()
            .take_while(|band| band_ratio >= band.enter_ratio)
            .count();
        let current_band = signal.band;
        // Leaving level 0 also requires `min_excess_copies`.
        let escalate =
            target_band > current_band && (current_band > 0 || excess >= config.min_excess_copies);
        // De-escalation drops to the highest band whose exit ratio the ratio still meets,
        // not one band down. Dropping one band at a time would make a collapsed ratio
        // wait out a second dwell in a lower band.
        let below_exit = current_band > 0 && band_ratio < config.bands[current_band - 1].exit_ratio;
        signal.below_exit = if below_exit {
            signal.below_exit + advance
        } else {
            0
        };
        let de_escalate = signal.below_exit >= config.exit_dwell;
        let new_band = if escalate {
            target_band
        } else if de_escalate {
            config
                .bands
                .iter()
                .take_while(|band| band_ratio >= band.exit_ratio)
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
        let ms_in_previous_band = signal.ms_in_band;
        if let Some(band) = transition {
            signal.band = band;
            signal.below_exit = 0;
            signal.commits_in_band = 0;
            signal.ms_in_band = 0;
            if self.enabled {
                if band > 0 {
                    // Set the cap before activating so that no submission uses the
                    // previous band's cap.
                    self.params.write().max_delay = config.bands[band - 1].max_delay;
                    self.set_active(true);
                } else {
                    // Deactivate before resetting the cap so that no active submission
                    // uses band 1's cap.
                    self.set_active(false);
                    self.params.write().max_delay = config.bands[0].max_delay;
                }
            }
            info!(
                "Duplication signal moved to {} (from {}): window duplication ratio \
                 {duplication_ratio:.4} ({excess} excess copies over {total} unique user \
                 transactions), {window_consensus_user_txns} user transactions in the window \
                 against a floor of {}, clock {:?}, window {}, exit dwell {}{}",
                signal_band_label(band),
                signal_band_label(current_band),
                config.min_consensus_user_txns,
                config.clock,
                config.window,
                config.exit_dwell,
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
            ms_in_previous_band,
            duplication_ratio,
            window_consensus_user_txns,
        }
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
/// the pool's capacities and metrics so slot requests carry no limits or metrics. The
/// stagger policy itself (mode, schedule) stays on the per-epoch [`StaggeredSubmission`]
/// in the epoch store.
///
/// Held submissions are not drained until their delay elapses, so without the quota
/// (`held_quota_percent` of `max_pending_transactions` entries and, when the pool has a
/// byte capacity, the same share of its bytes), sustained no-proposer-list inflow
/// at up to `max_delay` of dwell each could fill the pool's capacity and crowd out
/// restricted traffic. At either quota the caller gets the retriable
/// `StaggeredSubmissionHoldQuotaFull` error instead, and the client retries against
/// another validator, plausibly one of the transaction's free-slot validators.
pub struct StaggerQuota {
    quota: usize,
    held: Arc<AtomicUsize>,
    /// The quota on the serialized bytes of held entries, set when the pool has a byte
    /// capacity.
    byte_quota: Option<u64>,
    held_bytes: Arc<AtomicU64>,
    /// Kept in lockstep with `held` for observability.
    metric: IntGauge,
    /// Kept in lockstep with `held_bytes` for observability.
    bytes_metric: IntGauge,
    /// Held submissions refused at a quota, labeled by the quota that refused them.
    quota_rejected_metric: IntCounterVec,
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
        max_pending_bytes: Option<u64>,
        held_quota_percent: u8,
        metrics: &AdmissionQueueMetrics,
    ) -> Self {
        assert!(
            (1..=100).contains(&held_quota_percent),
            "staggered held quota percent must be between 1 and 100, got {held_quota_percent}"
        );
        let percent = u128::from(held_quota_percent);
        Self {
            quota: ((max_pending_transactions as u128 * percent / 100) as usize).max(1),
            held: Arc::new(AtomicUsize::new(0)),
            byte_quota: max_pending_bytes
                .map(|bytes| ((u128::from(bytes) * percent / 100) as u64).max(1)),
            held_bytes: Arc::new(AtomicU64::new(0)),
            metric: metrics.pool_staggered_held.clone(),
            bytes_metric: metrics.pool_staggered_held_bytes.clone(),
            quota_rejected_metric: metrics.pool_staggered_quota_rejected.clone(),
            rejected_metric: metrics.pool_staggered_rejected.clone(),
            shadow_decisions_metric: metrics.pool_stagger_shadow_decisions.clone(),
            shadow_delay_metric: metrics.pool_stagger_shadow_delay.clone(),
        }
    }

    #[cfg(test)]
    pub fn byte_quota(&self) -> Option<u64> {
        self.byte_quota
    }

    /// Like [`StaggeredSubmission::submission_delay`], but also registers the hold
    /// against the quota and returns it as a [`StaggeredSlot`] carrying the
    /// eligibility time. `bytes` is the submission's serialized size, which the hold
    /// counts against the byte quota. A submission outside the proposer set is
    /// rejected before it touches the quota.
    pub fn submission_slot(
        &self,
        txs: &[&Transaction],
        epoch_store: &AuthorityPerEpochStore,
        bytes: u64,
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
        self.hold(delay, bytes).map(Some)
    }

    /// Registers a hold of `bytes` against both quotas. The byte total is checked
    /// before the hold is added, so one hold can take it above the byte quota by at
    /// most its own size. A held submission of any valid size is therefore admitted
    /// while the byte quota has room.
    fn hold(&self, delay: Duration, bytes: u64) -> SuiResult<StaggeredSlot> {
        if self
            .held
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |held| {
                (held < self.quota).then_some(held + 1)
            })
            .is_err()
        {
            self.quota_rejected_metric
                .with_label_values(&["entries"])
                .inc();
            return Err(SuiErrorKind::StaggeredSubmissionHoldQuotaFull.into());
        }
        if self
            .held_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |held| {
                self.byte_quota
                    .is_none_or(|quota| held < quota)
                    .then_some(held + bytes)
            })
            .is_err()
        {
            self.held.fetch_sub(1, Ordering::Relaxed);
            self.quota_rejected_metric
                .with_label_values(&["bytes"])
                .inc();
            return Err(SuiErrorKind::StaggeredSubmissionHoldQuotaFull.into());
        }
        self.metric.inc();
        self.bytes_metric.add(bytes as i64);
        Ok(StaggeredSlot {
            delay,
            eligible_at: Instant::now() + delay,
            bytes,
            held: self.held.clone(),
            held_bytes: self.held_bytes.clone(),
            metric: self.metric.clone(),
            bytes_metric: self.bytes_metric.clone(),
            released: false,
        })
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
    /// The serialized bytes the hold counts against the byte quota.
    bytes: u64,
    held: Arc<AtomicUsize>,
    held_bytes: Arc<AtomicU64>,
    metric: IntGauge,
    bytes_metric: IntGauge,
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
        self.held_bytes.fetch_add(self.bytes, Ordering::Relaxed);
        self.metric.inc();
        self.bytes_metric.add(self.bytes as i64);
    }

    fn dec(&self) {
        let previous = self.held.fetch_sub(1, Ordering::Relaxed);
        debug_assert!(previous > 0, "staggered held count underflow");
        let previous_bytes = self.held_bytes.fetch_sub(self.bytes, Ordering::Relaxed);
        debug_assert!(
            previous_bytes >= self.bytes,
            "staggered held bytes underflow"
        );
        self.metric.dec();
        self.bytes_metric.sub(self.bytes as i64);
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
pub(crate) fn stagger_seed(
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

/// The stake-weighted permutation of the committee derived from `seed`: members are
/// drawn without replacement with probability proportional to voting power, the same
/// primitive as `Committee::shuffle_by_stake_from_tx_digest` and the consensus leader
/// schedule. Weighting by stake makes early-slot honesty track honest *stake* (the BFT
/// assumption) rather than validator count, makes splitting stake across seats buy no
/// extra slot-0 share, and lands the implied submission load on validators in
/// proportion to their stake.
///
/// Validators read their own slot out of this order and the transaction driver reads
/// which validators hold the leading slots; defining both against the same function
/// keeps the two sides of the schedule from ever diverging.
pub(crate) fn stagger_order(seed: &[u8; 32], committee: &Committee) -> Vec<AuthorityName> {
    let mut rng = StdRng::from_seed(*seed);
    committee.shuffle_by_stake_with_rng(None, None, &mut rng)
}

/// This validator's slot in the [`stagger_order`] permutation.
#[cfg(test)]
pub(crate) fn stagger_slot(seed: &[u8; 32], committee: &Committee, own_index: u32) -> u64 {
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
pub(crate) fn stagger_proposer_slots(order: &[AuthorityName], committee: &Committee) -> u64 {
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
    use rstest::rstest;

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

    /// Milliseconds between the consensus timestamps of consecutive test commits.
    const TEST_COMMIT_INTERVAL_MS: u64 = 100;
    const TEST_MAX_COMMIT_GAP_MS: u64 = 10 * TEST_COMMIT_INTERVAL_MS;

    impl StaggeredSubmission {
        /// Calls `record_commit` for a commit whose only transactions are its unique
        /// transactions and its excess copies.
        fn record(&self, excess_copies: u64, unique_user_txns: u64) -> CommitSignal {
            self.record_txns(
                excess_copies,
                unique_user_txns,
                excess_copies + unique_user_txns,
            )
        }

        /// Calls `record_commit` for a commit `TEST_COMMIT_INTERVAL_MS` after the
        /// previous one.
        fn record_txns(
            &self,
            excess_copies: u64,
            unique_user_txns: u64,
            consensus_user_txns: u64,
        ) -> CommitSignal {
            self.record_txns_after(
                TEST_COMMIT_INTERVAL_MS,
                excess_copies,
                unique_user_txns,
                consensus_user_txns,
            )
        }

        /// Calls `record_commit` for a commit `interval_ms` after the largest timestamp
        /// recorded so far. The first commit is at timestamp 0.
        fn record_txns_after(
            &self,
            interval_ms: u64,
            excess_copies: u64,
            unique_user_txns: u64,
            consensus_user_txns: u64,
        ) -> CommitSignal {
            let timestamp_ms = self
                .signal
                .lock()
                .last_commit_timestamp_ms
                .map_or(0, |previous| previous + interval_ms);
            self.record_commit(
                timestamp_ms,
                excess_copies,
                unique_user_txns,
                consensus_user_txns,
            )
        }
    }

    /// The signal clock of a test config. The time clock's window and dwell span the
    /// same number of test commits as the commit clock's, so a test that records
    /// commits `TEST_COMMIT_INTERVAL_MS` apart sees the same transitions on both.
    #[derive(Debug, Clone, Copy)]
    enum TestClock {
        Commits,
        Time,
    }

    fn test_signal_config(clock: TestClock) -> SignalConfig {
        let (clock, unit) = match clock {
            TestClock::Commits => (SignalClock::Commits, 1),
            TestClock::Time => (
                SignalClock::CommitTimeMs {
                    max_commit_gap_ms: TEST_MAX_COMMIT_GAP_MS,
                },
                TEST_COMMIT_INTERVAL_MS,
            ),
        };
        SignalConfig {
            clock,
            window: TEST_SIGNAL_WINDOW_COMMITS as u64 * unit,
            min_excess_copies: TEST_SIGNAL_MIN_EXCESS_COPIES,
            min_consensus_user_txns: 0,
            exit_dwell: TEST_SIGNAL_EXIT_DWELL_COMMITS as u64 * unit,
            bands: TEST_SIGNAL_BANDS,
        }
    }

    /// The volume floor is 100 user transactions per window commit.
    const TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS: u64 = 100 * TEST_SIGNAL_WINDOW_COMMITS as u64;

    fn test_signal_config_with_volume_floor(clock: TestClock) -> SignalConfig {
        SignalConfig {
            min_consensus_user_txns: TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS,
            ..test_signal_config(clock)
        }
    }

    #[test]
    fn signal_config_is_valid_for_every_protocol_version() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        SignalConfig::default().assert_valid();
        test_signal_config(TestClock::Commits).assert_valid();
        test_signal_config(TestClock::Time).assert_valid();
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
        let mut config = test_signal_config(TestClock::Commits);
        config.bands.swap(0, 1);
        config.assert_valid();
    }

    #[test]
    #[should_panic(expected = "exit ratio must be below")]
    fn signal_config_rejects_exit_ratio_above_enter_ratio() {
        let mut config = test_signal_config(TestClock::Commits);
        config.bands[0].exit_ratio = config.bands[0].enter_ratio;
        config.assert_valid();
    }

    #[test]
    #[should_panic(expected = "max delay must not be below")]
    fn signal_config_rejects_decreasing_max_delay() {
        let mut config = test_signal_config(TestClock::Commits);
        config.bands[1].max_delay = config.bands[0].max_delay / 2;
        config.assert_valid();
    }

    #[test]
    #[should_panic(expected = "exit dwell must be positive")]
    fn signal_config_rejects_zero_exit_dwell() {
        let mut config = test_signal_config(TestClock::Commits);
        config.exit_dwell = 0;
        config.assert_valid();
    }

    #[test]
    #[should_panic(expected = "max commit gap must be positive")]
    fn signal_config_rejects_zero_max_commit_gap() {
        let mut config = test_signal_config(TestClock::Time);
        config.clock = SignalClock::CommitTimeMs {
            max_commit_gap_ms: 0,
        };
        config.assert_valid();
    }

    /// Runs `from_protocol_config` and returns its panic message, if it panicked.
    fn from_protocol_config_panic(config: &sui_protocol_config::ProtocolConfig) -> Option<String> {
        let panic =
            std::panic::catch_unwind(|| SignalConfig::from_protocol_config(config)).err()?;
        Some(
            panic
                .downcast_ref::<&str>()
                .map(|message| message.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default(),
        )
    }

    #[test]
    fn signal_config_rejects_mixed_clock_fields() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        type Set = fn(&mut ProtocolConfig);
        let commit_fields: [(&str, Set); 4] = [
            ("window_commits", |config| {
                config.set_staggered_submission_signal_window_commits_for_testing(600)
            }),
            ("min_excess_copies_per_commit", |config| {
                config.set_staggered_submission_signal_min_excess_copies_per_commit_for_testing(2)
            }),
            ("min_consensus_user_transactions_per_commit", |config| {
                config
                    .set_staggered_submission_signal_min_consensus_user_transactions_per_commit_for_testing(
                        1_000,
                    )
            }),
            ("exit_dwell_commits", |config| {
                config.set_staggered_submission_signal_exit_dwell_commits_for_testing(6_000)
            }),
        ];
        let time_fields: [(&str, Set); 4] = [
            ("min_excess_copies_per_second", |config| {
                config.set_staggered_submission_signal_min_excess_copies_per_second_for_testing(30)
            }),
            ("min_consensus_user_transactions_per_second", |config| {
                config
                    .set_staggered_submission_signal_min_consensus_user_transactions_per_second_for_testing(
                        12_000,
                    )
            }),
            ("exit_dwell_ms", |config| {
                config.set_staggered_submission_signal_exit_dwell_ms_for_testing(400_000)
            }),
            ("max_commit_gap_ms", |config| {
                config.set_staggered_submission_signal_max_commit_gap_ms_for_testing(5_000)
            }),
        ];
        for (field, set) in commit_fields {
            let mut config = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Mainnet);
            set(&mut config);
            let message = from_protocol_config_panic(&config).expect(field);
            assert!(
                message.contains("needs the commit-based signal fields unset"),
                "{field}: {message}"
            );
        }
        for (field, set) in time_fields {
            let mut config =
                ProtocolConfig::get_for_version(ProtocolVersion::new(144), Chain::Mainnet);
            set(&mut config);
            let message = from_protocol_config_panic(&config).expect(field);
            assert!(
                message.contains("need staggered_submission_signal_window_ms"),
                "{field}: {message}"
            );
        }
    }

    #[test]
    fn signal_config_requires_thresholds_when_the_signal_is_on() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        let mut config = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Mainnet);
        config.disable_staggered_submission_signal_window_ms_for_testing();
        config.disable_staggered_submission_signal_min_excess_copies_per_second_for_testing();
        config
            .disable_staggered_submission_signal_min_consensus_user_transactions_per_second_for_testing();
        config.disable_staggered_submission_signal_exit_dwell_ms_for_testing();
        config.disable_staggered_submission_signal_max_commit_gap_ms_for_testing();
        let message = from_protocol_config_panic(&config).expect("no thresholds accepted");
        assert!(
            message.contains("needs staggered_submission_signal_window_ms or the commit-based"),
            "{message}"
        );
        // With the signal off the defaults stay acceptable.
        config.set_staggered_submission_signal_for_testing(false);
        assert_eq!(
            SignalConfig::from_protocol_config(&config),
            SignalConfig::default()
        );

        // Each of the commit-based window, excess-copy floor and dwell is required.
        type Disable = fn(&mut ProtocolConfig);
        let disables: [(&str, Disable); 3] = [
            (
                "window_commits",
                ProtocolConfig::disable_staggered_submission_signal_window_commits_for_testing,
            ),
            (
                "min_excess_copies_per_commit",
                ProtocolConfig::disable_staggered_submission_signal_min_excess_copies_per_commit_for_testing,
            ),
            (
                "exit_dwell_commits",
                ProtocolConfig::disable_staggered_submission_signal_exit_dwell_commits_for_testing,
            ),
        ];
        for (field, disable) in disables {
            let mut config =
                ProtocolConfig::get_for_version(ProtocolVersion::new(144), Chain::Mainnet);
            disable(&mut config);
            let message = from_protocol_config_panic(&config).expect(field);
            assert!(
                message.contains("needs staggered_submission_signal_window_ms or the commit-based"),
                "{field}: {message}"
            );
        }
    }

    #[test]
    fn signal_config_rejects_a_gap_cap_not_below_the_window_or_dwell() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        let latest = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Mainnet);
        // (window, dwell, cap): a cap equal to the window, and a cap below the window
        // but equal to a dwell shorter than the window.
        for (window_ms, exit_dwell_ms, cap) in [(40_000, 400_000, 40_000), (40_000, 10_000, 10_000)]
        {
            let mut config = latest.clone();
            config.set_staggered_submission_signal_window_ms_for_testing(window_ms);
            config.set_staggered_submission_signal_exit_dwell_ms_for_testing(exit_dwell_ms);
            config.set_staggered_submission_signal_max_commit_gap_ms_for_testing(cap);
            let message = from_protocol_config_panic(&config).expect("cap accepted");
            assert!(
                message.contains("must be below the window and the exit dwell"),
                "{cap}: {message}"
            );
        }
    }

    #[test]
    #[should_panic(expected = "signal window must be positive")]
    fn signal_config_rejects_zero_window_commits() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        let mut config = ProtocolConfig::get_for_version(ProtocolVersion::new(144), Chain::Mainnet);
        config.set_staggered_submission_signal_window_commits_for_testing(0);
        SignalConfig::from_protocol_config(&config);
    }

    #[test]
    fn signal_config_requires_every_time_field() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        type Disable = fn(&mut ProtocolConfig);
        let disables: [(&str, Disable); 4] = [
            (
                "staggered_submission_signal_max_commit_gap_ms",
                ProtocolConfig::disable_staggered_submission_signal_max_commit_gap_ms_for_testing,
            ),
            (
                "staggered_submission_signal_min_excess_copies_per_second",
                ProtocolConfig::disable_staggered_submission_signal_min_excess_copies_per_second_for_testing,
            ),
            (
                "staggered_submission_signal_min_consensus_user_transactions_per_second",
                ProtocolConfig::disable_staggered_submission_signal_min_consensus_user_transactions_per_second_for_testing,
            ),
            (
                "staggered_submission_signal_exit_dwell_ms",
                ProtocolConfig::disable_staggered_submission_signal_exit_dwell_ms_for_testing,
            ),
        ];
        for (field, disable) in disables {
            let mut config = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Mainnet);
            disable(&mut config);
            let message = from_protocol_config_panic(&config).expect(field);
            assert!(
                message.contains(&format!("the time clock needs {field}")),
                "{field}: {message}"
            );
        }
    }

    #[test]
    #[should_panic(expected = "signal window must be positive")]
    fn signal_config_rejects_zero_window_ms() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        let mut config = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Mainnet);
        config.set_staggered_submission_signal_window_ms_for_testing(0);
        SignalConfig::from_protocol_config(&config);
    }

    #[test]
    fn time_thresholds_match_version_144_at_15_commits_per_second() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        const COMMITS_PER_SECOND: u64 = 15;
        let v145 = SignalConfig::from_protocol_config(&ProtocolConfig::get_for_version(
            ProtocolVersion::new(145),
            Chain::Mainnet,
        ));
        let v144 = SignalConfig::from_protocol_config(&ProtocolConfig::get_for_version(
            ProtocolVersion::new(144),
            Chain::Mainnet,
        ));
        assert_eq!(v145.window * COMMITS_PER_SECOND / 1_000, v144.window);
        assert_eq!(
            v145.exit_dwell * COMMITS_PER_SECOND / 1_000,
            v144.exit_dwell
        );
        assert_eq!(v145.min_excess_copies, v144.min_excess_copies);
        assert_eq!(v145.bands, v144.bands);
        // The v145 volume floor is 12,000 user transactions per second. At 15 commits
        // per second v144's floor is 15,000 per second.
        assert_eq!(
            v145.min_consensus_user_txns * 15_000 / 12_000,
            v144.min_consensus_user_txns
        );
        // Mainnet commits are about 65 ms apart, and about 500 ms apart with validators
        // down. The cap must stay above that.
        let SignalClock::CommitTimeMs { max_commit_gap_ms } = v145.clock else {
            panic!("v145 uses the commit clock");
        };
        assert!(max_commit_gap_ms >= 500, "{max_commit_gap_ms}");

        // On testnet the v145 volume floor equals v144's.
        let testnet = |version| {
            SignalConfig::from_protocol_config(&ProtocolConfig::get_for_version(
                ProtocolVersion::new(version),
                Chain::Testnet,
            ))
        };
        assert_eq!(
            testnet(145).min_consensus_user_txns,
            testnet(144).min_consensus_user_txns
        );
        // Devnet and local networks use the mainnet values.
        assert_eq!(
            SignalConfig::from_protocol_config(&ProtocolConfig::get_for_version(
                ProtocolVersion::new(145),
                Chain::Unknown,
            )),
            v145
        );
    }

    #[test]
    #[should_panic(expected = "max commit gap must be positive")]
    fn signal_config_rejects_zero_max_commit_gap_ms() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        let mut config = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Mainnet);
        config.set_staggered_submission_signal_max_commit_gap_ms_for_testing(0);
        SignalConfig::from_protocol_config(&config);
    }

    #[test]
    fn time_clock_floors_round_down_after_multiplying() {
        use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};

        let mut config = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Mainnet);
        config.set_staggered_submission_signal_window_ms_for_testing(2_500);
        config.set_staggered_submission_signal_max_commit_gap_ms_for_testing(1_000);
        config.set_staggered_submission_signal_min_excess_copies_per_second_for_testing(3);
        config
            .set_staggered_submission_signal_min_consensus_user_transactions_per_second_for_testing(
                1,
            );
        let signal_config = SignalConfig::from_protocol_config(&config);
        // 3 * 2,500 / 1,000 rounds 7.5 down to 7. Rounding the window to whole seconds
        // first would give 6.
        assert_eq!(signal_config.min_excess_copies, 7);
        // 1 * 2,500 / 1,000 rounds 2.5 down to 2.
        assert_eq!(signal_config.min_consensus_user_txns, 2);
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

        // Version 145 moves the signal to the time clock.
        let latest = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Mainnet);
        let config = SignalConfig::from_protocol_config(&latest);
        let window_ms = latest.staggered_submission_signal_window_ms();
        assert_eq!(
            config.clock,
            SignalClock::CommitTimeMs {
                max_commit_gap_ms: latest.staggered_submission_signal_max_commit_gap_ms()
            }
        );
        assert_eq!(config.window, window_ms);
        assert_eq!(
            config.min_excess_copies,
            latest.staggered_submission_signal_min_excess_copies_per_second() * window_ms / 1_000
        );
        assert_eq!(
            config.min_consensus_user_txns,
            latest.staggered_submission_signal_min_consensus_user_transactions_per_second()
                * window_ms
                / 1_000
        );
        assert_eq!(
            config.exit_dwell,
            latest.staggered_submission_signal_exit_dwell_ms()
        );
        assert!(
            latest
                .staggered_submission_signal_window_commits_as_option()
                .is_none()
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

        // Testnet has its own volume floor and the mainnet values otherwise.
        let testnet = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Testnet);
        let testnet_rate =
            testnet.staggered_submission_signal_min_consensus_user_transactions_per_second();
        assert!(
            testnet_rate
                < latest.staggered_submission_signal_min_consensus_user_transactions_per_second()
        );
        assert_eq!(
            SignalConfig::from_protocol_config(&testnet),
            SignalConfig {
                min_consensus_user_txns: testnet_rate * window_ms / 1_000,
                ..config.clone()
            }
        );

        // Version 144 uses the commit clock with per-commit floors.
        let v144 = ProtocolConfig::get_for_version(ProtocolVersion::new(144), Chain::Mainnet);
        let config = SignalConfig::from_protocol_config(&v144);
        let window = v144.staggered_submission_signal_window_commits();
        assert_eq!(config.clock, SignalClock::Commits);
        assert_eq!(config.window, window);
        assert_eq!(
            config.min_excess_copies,
            v144.staggered_submission_signal_min_excess_copies_per_commit() * window
        );
        assert_eq!(
            config.min_consensus_user_txns,
            v144.staggered_submission_signal_min_consensus_user_transactions_per_commit() * window
        );
        assert_eq!(
            config.exit_dwell,
            v144.staggered_submission_signal_exit_dwell_commits()
        );

        // Version 143 has the thresholds of version 140, which equal the defaults, and
        // no volume floor.
        let no_floor = ProtocolConfig::get_for_version(ProtocolVersion::new(143), Chain::Mainnet);
        assert!(
            no_floor
                .staggered_submission_signal_window_commits_as_option()
                .is_some()
        );
        assert_eq!(
            SignalConfig::from_protocol_config(&no_floor),
            SignalConfig::default()
        );
        assert_eq!(SignalConfig::default().min_consensus_user_txns, 0);

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

    #[rstest]
    fn record_commit_reports_the_dwell_of_the_band_left(
        #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
    ) {
        let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
        let first = staggered.record(1000, 2000);
        assert_eq!(first.transition, Some(2));
        assert_eq!(first.previous_band, 0);
        assert_eq!(first.commits_in_previous_band, 1);

        // The activating commit counts towards band 0.
        let mut recorded_in_band2 = 0;
        let mut left = None;
        for _ in 0..=TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
            let outcome = staggered.record(0, 100);
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
        // Each commit after the activation is `TEST_COMMIT_INTERVAL_MS` after the one
        // before, whichever clock the signal uses.
        assert_eq!(
            left.ms_in_previous_band,
            recorded_in_band2 * TEST_COMMIT_INTERVAL_MS
        );
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
    fn order_and_slots_agree() {
        let seed = test_seed(7);
        let (committee, _) = Committee::new_simple_test_committee_of_size(7);
        let order = stagger_order(&seed, &committee);
        for index in 0..committee.num_members() as u32 {
            let name = committee.authority_by_index(index).unwrap();
            assert_eq!(
                order[stagger_slot(&seed, &committee, index) as usize],
                *name
            );
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

        #[rstest]
        fn duplication_ratio_tracks_the_window(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(staggered.record(10, 100).duplication_ratio, 0.1);
            assert_eq!(staggered.record(0, 100).duplication_ratio, 0.05);
            // A full window of quiet commits removes the spike.
            let mut ratio = f64::NAN;
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS {
                ratio = staggered.record(0, 100).duplication_ratio;
            }
            assert_eq!(ratio, 0.0);
        }

        #[rstest]
        #[tokio::test]
        async fn late_copies_activate_without_new_unique_transactions(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            staggered.record(0, 1);
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record(0, 0).pair(), (None, 0.0));
            }

            // `min_excess_copies` still applies when the ratio is large.
            assert_eq!(
                staggered
                    .record(TEST_SIGNAL_MIN_EXCESS_COPIES - 1, 0)
                    .pair(),
                (None, (TEST_SIGNAL_MIN_EXCESS_COPIES - 1) as f64),
            );
            assert!(!staggered.is_active());
            assert_eq!(
                staggered.record(1, 0).pair(),
                (Some(2), TEST_SIGNAL_MIN_EXCESS_COPIES as f64),
            );
            assert!(staggered.is_active());
            assert_eq!(
                staggered.params.read().max_delay,
                TEST_SIGNAL_BANDS[1].max_delay
            );
        }

        #[rstest]
        fn burst_escalates_to_the_matching_band(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            // A 7% burst enters band 1.
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(staggered.record(700, 10000).transition, Some(1));
            assert!(staggered.is_active());
            // A 50% burst enters band 2 directly.
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(staggered.record(1000, 2000).transition, Some(2));
            assert!(staggered.is_active());
        }

        #[rstest]
        fn escalates_further_as_the_ratio_climbs(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(staggered.record(700, 10000).transition, Some(1));
            // More copies in band 1 push the ratio over band 2's enter ratio.
            assert_eq!(staggered.record(3000, 1000).transition, Some(2));
            assert!(staggered.is_active());
        }

        #[rstest]
        fn applied_transitions_retune_the_max_delay(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(
                staggered.params.read().max_delay,
                TEST_SIGNAL_BANDS[0].max_delay
            );
            assert_eq!(staggered.record(1000, 2000).transition, Some(2));
            assert_eq!(
                staggered.params.read().max_delay,
                TEST_SIGNAL_BANDS[1].max_delay
            );
            // A steady 4% brings the ratio into band 1's range. After the dwell the
            // signal moves to band 1 and lowers the cap.
            let mut transitions = Vec::new();
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record(4, 100).transition);
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

        #[rstest]
        fn transition_to_off_resets_the_max_delay_to_band_one(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(staggered.record(1000, 2000).transition, Some(2));
            let mut transitions = Vec::new();
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record(0, 100).transition);
                if !transitions.is_empty() {
                    break;
                }
            }
            assert_eq!(transitions, vec![0]);
            assert!(!staggered.is_active());
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

        #[rstest]
        fn ratio_below_threshold_does_not_activate(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            // At 4% per commit the excess copies pass `min_excess_copies`, but the
            // ratio stays below band 1.
            for _ in 0..30 {
                assert_eq!(staggered.record(40, 1000).transition, None);
            }
            assert!(!staggered.is_active());
        }

        #[rstest]
        fn floor_blocks_high_ratio_at_low_volume(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            // The ratio is 20%, with two excess copies per commit. The signal waits
            // until a full window reaches `min_excess_copies` and then enters band 2.
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS - 1 {
                assert_eq!(staggered.record(2, 10).transition, None);
                assert!(!staggered.is_active());
            }
            assert_eq!(staggered.record(2, 10).transition, Some(2));
        }

        #[rstest]
        fn low_volume_window_does_not_activate(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered =
                StaggeredSubmission::new(test_signal_config_with_volume_floor(clock), true);
            // Each commit has 60 excess copies over 30 unique transactions. That passes
            // `min_excess_copies` and band 2's ratio, with 90 user transactions per commit.
            for _ in 0..2 * TEST_SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record(60, 30).pair(), (None, 2.0));
            }
            assert!(!staggered.is_active());
            assert_eq!(staggered.signal_band(), 0);
        }

        #[rstest]
        fn volume_floor_counts_unique_transactions_and_every_duplicate(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered =
                StaggeredSubmission::new(test_signal_config_with_volume_floor(clock), true);
            // 90 user transactions per commit are excess or unique. Another 10 are paid or
            // restricted duplicates, which reach the floor once the window is full.
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS - 1 {
                assert_eq!(staggered.record_txns(60, 30, 100).transition, None);
            }
            assert_eq!(staggered.record_txns(60, 30, 100).transition, Some(2));
            assert!(staggered.is_active());
        }

        #[rstest]
        fn volume_floor_is_a_window_total(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            // One commit that carries a full window's worth of user transactions passes the
            // floor.
            let staggered =
                StaggeredSubmission::new(test_signal_config_with_volume_floor(clock), true);
            let unique = TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS / 3;
            assert_eq!(
                staggered
                    .record_txns(
                        TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS - unique,
                        unique,
                        TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS
                    )
                    .transition,
                Some(2)
            );
            // One transaction fewer does not.
            let staggered =
                StaggeredSubmission::new(test_signal_config_with_volume_floor(clock), true);
            assert_eq!(
                staggered
                    .record_txns(
                        TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS - unique - 1,
                        unique,
                        TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS - 1
                    )
                    .transition,
                None
            );
        }

        /// A commit that enters band 2 at a 200% ratio and carries three windows' worth
        /// of user transactions.
        fn enter_band2_with_burst(staggered: &StaggeredSubmission) {
            let outcome = staggered.record_txns(
                2 * TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS,
                TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS,
                3 * TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS,
            );
            assert_eq!(outcome.transition, Some(2));
        }

        /// Commits at a 200% ratio and 90 user transactions each that it takes to turn band 2 off,
        /// starting right after a burst. The burst keeps the window above the floor
        /// until it leaves the window. The dwell runs from that commit.
        const LOW_VOLUME_COMMITS_TO_OFF: usize =
            TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS - 1;

        #[rstest]
        fn low_volume_turns_the_signal_off_after_the_dwell(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered =
                StaggeredSubmission::new(test_signal_config_with_volume_floor(clock), true);
            enter_band2_with_burst(&staggered);
            // The ratio stays at 200%, above every exit ratio. Only the volume is low.
            for _ in 0..LOW_VOLUME_COMMITS_TO_OFF - 1 {
                assert_eq!(staggered.record(60, 30).pair(), (None, 2.0));
                assert!(staggered.is_active());
            }
            // The signal drops from band 2 straight to off.
            assert_eq!(staggered.record(60, 30).pair(), (Some(0), 2.0));
            assert!(!staggered.is_active());
            // The same traffic does not turn the signal back on.
            for _ in 0..2 * TEST_SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record(60, 30).transition, None);
            }
        }

        #[rstest]
        fn volume_dip_shorter_than_the_dwell_keeps_the_band(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered =
                StaggeredSubmission::new(test_signal_config_with_volume_floor(clock), true);
            enter_band2_with_burst(&staggered);
            for _ in 0..LOW_VOLUME_COMMITS_TO_OFF - 1 {
                assert_eq!(staggered.record(60, 30).transition, None);
            }
            // One commit before the dwell ends, a commit lifts the window over the
            // floor again. The dwell restarts.
            assert_eq!(
                staggered
                    .record_txns(
                        2 * TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS,
                        TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS,
                        3 * TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS
                    )
                    .transition,
                None
            );
            for _ in 0..LOW_VOLUME_COMMITS_TO_OFF - 1 {
                assert_eq!(staggered.record(60, 30).transition, None);
            }
            assert_eq!(staggered.signal_band(), 2);
            assert_eq!(staggered.record(60, 30).transition, Some(0));
        }

        #[rstest]
        fn dwell_that_ends_above_the_floor_follows_the_exit_ratios(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered =
                StaggeredSubmission::new(test_signal_config_with_volume_floor(clock), true);
            enter_band2_with_burst(&staggered);
            for _ in 0..LOW_VOLUME_COMMITS_TO_OFF - 1 {
                assert_eq!(staggered.record(60, 30).transition, None);
            }
            // The window holds 299 low-volume commits: 17,940 excess copies over 8,970
            // unique transactions. The last commit of the dwell brings 170,430 unique
            // transactions without duplicates. The window is then over the floor, and
            // the ratio is 10%, below band 2's exit ratio and above band 1's. The signal
            // moves to band 1, not to off.
            assert_eq!(
                staggered.record_txns(0, 170_430, 170_430).pair(),
                (Some(1), 0.1)
            );
            assert!(staggered.is_active());
        }

        #[rstest]
        fn low_volume_window_does_not_escalate_an_active_band(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered =
                StaggeredSubmission::new(test_signal_config_with_volume_floor(clock), true);
            // A 7% burst with enough volume enters band 1.
            assert_eq!(
                staggered
                    .record_txns(
                        7 * TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS / 100,
                        TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS,
                        2 * TEST_SIGNAL_MIN_CONSENSUS_USER_TXNS
                    )
                    .transition,
                Some(1)
            );
            // Commits without duplicates move the burst out of the window.
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record(0, 30).transition, None);
            }
            // The ratio then climbs to 200% at low volume. The signal does not move to
            // band 2. It turns off when the dwell ends.
            let mut transitions = Vec::new();
            for _ in 0..TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record(60, 30).transition);
                assert!(staggered.signal_band() <= 1);
            }
            assert_eq!(transitions, vec![0]);
        }

        #[rstest]
        fn old_spikes_slide_out_of_window(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            // 500 excess copies at 50% stay below `min_excess_copies`.
            assert_eq!(staggered.record(500, 1000).transition, None);
            // After a full window of quiet commits, a second spike does not add to
            // the first.
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record(0, 100).transition, None);
            }
            assert_eq!(staggered.record(500, 1000).transition, None);
            assert!(!staggered.is_active());
        }

        #[rstest]
        fn deescalates_straight_to_off_once_quiet_traffic_dilutes_the_spike(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(staggered.record(1000, 2000).transition, Some(2));
            // Quiet traffic brings the ratio to zero. After the dwell the signal drops
            // from band 2 straight to off, without visiting band 1.
            let mut transitions = Vec::new();
            for _ in 0..=TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record(0, 100).transition);
                assert_eq!(staggered.is_active(), transitions.is_empty());
            }
            assert_eq!(transitions, vec![0]);
        }

        #[rstest]
        fn threshold_gaps_hold_the_band_between_exit_and_enter(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(staggered.record(1000, 2000).transition, Some(2));
            // A steady 4% lies between band 1's exit and enter ratios. The signal
            // moves to band 1 and stays there.
            let mut transitions = Vec::new();
            for _ in 0..2 * TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record(4, 100).transition);
                assert!(staggered.is_active());
            }
            assert_eq!(transitions, vec![1]);
            // After 2% turns the signal off, 4% does not turn it back on.
            let mut transitions = Vec::new();
            for _ in 0..TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record(2, 100).transition);
            }
            assert_eq!(transitions, vec![0]);
            for _ in 0..2 * TEST_SIGNAL_WINDOW_COMMITS {
                assert_eq!(staggered.record(4, 100).transition, None);
                assert!(!staggered.is_active());
            }
        }

        #[rstest]
        fn gap_holds_band2_between_exit_and_enter(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            // A steady 17% lies between band 2's exit and enter ratios. It keeps
            // band 2 active.
            // The run outlasts a window and two dwells, so a dwell that counted these
            // commits would end within it.
            let commits = 2 * TEST_SIGNAL_WINDOW_COMMITS + 2 * TEST_SIGNAL_EXIT_DWELL_COMMITS;
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(staggered.record(1000, 2000).transition, Some(2));
            for _ in 0..commits {
                assert_eq!(staggered.record(17, 100).transition, None);
            }
            // It also does not move band 1 up to band 2.
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(staggered.record(700, 10000).transition, Some(1));
            for _ in 0..commits {
                assert_eq!(staggered.record(17, 100).transition, None);
            }
        }

        #[rstest]
        fn dry_run_tracks_transitions_without_flipping_staggering(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), false);
            // A disabled signal reports transitions.
            assert_eq!(staggered.record(1000, 2000).transition, Some(2));
            // Staggering and its hold cap do not change.
            assert!(!staggered.is_active());
            assert_eq!(
                staggered.params.read().max_delay,
                TEST_SIGNAL_BANDS[0].max_delay
            );
            // De-escalation is also reported without changing staggering.
            let mut transitions = Vec::new();
            for _ in 0..=TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS {
                transitions.extend(staggered.record(0, 100).transition);
                assert!(!staggered.is_active());
            }
            assert_eq!(transitions, vec![0]);
            assert_eq!(
                staggered.params.read().max_delay,
                TEST_SIGNAL_BANDS[0].max_delay
            );
        }

        #[rstest]
        fn exit_dwell_absorbs_transient_dips(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(staggered.record(1000, 2000).transition, Some(2));
            let exit_ratio = TEST_SIGNAL_BANDS[1].exit_ratio;

            // Quiet traffic takes the ratio below band 2's exit ratio. The band stays
            // active for half the dwell.
            let mut below_exit = 0;
            while below_exit < TEST_SIGNAL_EXIT_DWELL_COMMITS / 2 {
                let (transition, ratio) = staggered.record(0, 100).pair();
                assert_eq!(transition, None);
                if ratio < exit_ratio {
                    below_exit += 1;
                }
            }
            // One heavy commit lifts the ratio above the exit ratio and restarts the
            // dwell.
            let (transition, ratio) = staggered.record(10_000, 100).pair();
            assert_eq!(transition, None);
            assert!(ratio >= exit_ratio);
            assert_eq!(staggered.signal_band(), 2);

            let mut below_exit = 0;
            loop {
                let (transition, ratio) = staggered.record(0, 100).pair();
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

        /// Records `commits` commits `interval_ms` apart, each with `excess` excess copies
        /// over `unique` unique transactions, and returns their transitions.
        fn record_spaced(
            staggered: &StaggeredSubmission,
            interval_ms: u64,
            commits: usize,
            excess: u64,
            unique: u64,
        ) -> Vec<Option<usize>> {
            (0..commits)
                .map(|_| {
                    staggered
                        .record_txns_after(interval_ms, excess, unique, excess + unique)
                        .transition
                })
                .collect()
        }

        #[test]
        fn time_clock_volume_floor_is_a_rate_per_second() {
            // The floor is 30,000 user transactions per 30 s window, or 1,000 per
            // second. Commits come every 200 ms, half the test rate, and carry a 200%
            // ratio.
            let below = |clock| {
                let staggered =
                    StaggeredSubmission::new(test_signal_config_with_volume_floor(clock), true);
                // 150 user transactions per commit is 750 per second.
                record_spaced(&staggered, 200, 2 * TEST_SIGNAL_WINDOW_COMMITS, 100, 50)
            };
            // The commit clock counts 300 commits and 45,000 user transactions, so the
            // lower rate passes its floor.
            assert!(below(TestClock::Commits).contains(&Some(2)));
            // The time clock counts 150 commits and 22,500 user transactions.
            assert!(below(TestClock::Time).iter().all(Option::is_none));

            // 210 user transactions per commit is 1,050 per second, which passes the
            // time clock's floor.
            let staggered = StaggeredSubmission::new(
                test_signal_config_with_volume_floor(TestClock::Time),
                true,
            );
            let transitions =
                record_spaced(&staggered, 200, 2 * TEST_SIGNAL_WINDOW_COMMITS, 140, 70);
            assert!(transitions.contains(&Some(2)));
        }

        #[test]
        fn time_clock_dwell_is_commit_time() {
            let staggered = StaggeredSubmission::new(test_signal_config(TestClock::Time), true);
            assert_eq!(staggered.record(1000, 2000).transition, Some(2));
            // Empty commits come every 400 ms, so the ratio stays at 50% until the
            // burst leaves the 30 s window on the 75th commit. The 60 s dwell then
            // takes 150 commits, the 75th of them included.
            let transitions = record_spaced(&staggered, 400, 300, 0, 0);
            let off = transitions
                .iter()
                .position(|transition| *transition == Some(0))
                .expect("signal never turned off");
            assert_eq!(off + 1, 75 + 150 - 1);
            assert_eq!(transitions.iter().flatten().count(), 1);
        }

        #[test]
        fn time_clock_caps_the_gap_after_a_stall() {
            let stall = |max_commit_gap_ms| {
                let staggered = StaggeredSubmission::new(
                    SignalConfig {
                        clock: SignalClock::CommitTimeMs { max_commit_gap_ms },
                        ..test_signal_config(TestClock::Time)
                    },
                    true,
                );
                assert_eq!(staggered.record(1000, 2000).transition, Some(2));
                // The burst leaves the window on the last of these commits, which starts
                // the dwell.
                // The commits are empty, so the ratio stays at 50% until then.
                let transitions = record_spaced(
                    &staggered,
                    TEST_COMMIT_INTERVAL_MS,
                    TEST_SIGNAL_WINDOW_COMMITS,
                    0,
                    0,
                );
                assert!(transitions.iter().all(Option::is_none));
                // Consensus stalls for ten minutes, longer than the window and the dwell.
                record_spaced(&staggered, 600_000, 1, 0, 0)[0]
            };
            // The dwell holds 100 ms before the stall, and the capped gap adds one
            // second.
            assert_eq!(stall(TEST_MAX_COMMIT_GAP_MS), None);
            // Without the cap the stall would end the dwell at once.
            assert_eq!(stall(u64::MAX), Some(0));
        }

        #[test]
        fn time_clock_keeps_the_window_across_a_stall() {
            let after_stall = |max_commit_gap_ms| {
                let staggered = StaggeredSubmission::new(
                    SignalConfig {
                        clock: SignalClock::CommitTimeMs { max_commit_gap_ms },
                        ..test_signal_config(TestClock::Time)
                    },
                    true,
                );
                assert_eq!(staggered.record(1000, 2000).transition, Some(2));
                // Consensus stalls for ten minutes, longer than the window.
                let signal = staggered.record_commit(600_000, 0, 100, 100);
                (
                    signal.window_consensus_user_txns,
                    staggered.signal.lock().window.len(),
                )
            };
            // The capped gap keeps the burst in the window.
            assert_eq!(after_stall(TEST_MAX_COMMIT_GAP_MS), (3100, 2));
            // Without the cap the stall would empty the window.
            assert_eq!(after_stall(u64::MAX), (100, 1));
        }

        #[test]
        fn time_clock_does_not_count_time_twice_after_a_timestamp_decreases() {
            let staggered = StaggeredSubmission::new(test_signal_config(TestClock::Time), true);
            for timestamp_ms in [1_000, 1_800, 1_100, 1_900] {
                staggered.record_commit(timestamp_ms, 0, 100, 100);
            }
            assert_eq!(staggered.signal.lock().clock, 900);
        }

        #[test]
        fn commit_clock_ignores_commit_timestamps() {
            // Equal, decreasing and very large gaps between timestamps.
            let irregular = |i: usize| match i % 3 {
                0 | 1 => 5_000,
                _ => 1_000_000 * i as u64,
            };
            let even = |i: usize| i as u64 * TEST_COMMIT_INTERVAL_MS;
            let run = |timestamp_ms: &dyn Fn(usize) -> u64| {
                let staggered =
                    StaggeredSubmission::new(test_signal_config(TestClock::Commits), true);
                let mut signals = vec![staggered.record_commit(timestamp_ms(0), 1000, 2000, 3000)];
                for i in 1..=TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS + 10 {
                    signals.push(staggered.record_commit(timestamp_ms(i), 0, 100, 100));
                }
                signals
                    .into_iter()
                    .map(|signal| {
                        (
                            signal.transition,
                            signal.commits_in_previous_band,
                            signal.duplication_ratio,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let evenly_spaced = run(&even);
            assert!(
                evenly_spaced
                    .iter()
                    .any(|(transition, ..)| *transition == Some(0))
            );
            assert_eq!(run(&irregular), evenly_spaced);
        }

        #[rstest]
        fn de_escalation_resets_the_dwell(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            assert_eq!(staggered.record(1000, 2000).transition, Some(2));
            // A steady 4% is below band 2's exit ratio and above band 1's. Band 2's
            // dwell ends with the signal in band 1. The test stops there, because a
            // further 4% commit would reset the dwell on its own.
            let moved_to_band1 = (0..TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS)
                .map(|_| staggered.record(4, 100).transition)
                .find(Option::is_some)
                .flatten();
            assert_eq!(moved_to_band1, Some(1));
            // A commit that takes the ratio below band 1's exit ratio starts band 1's
            // dwell. It does not continue band 2's.
            let signal = staggered.record(0, 100_000);
            assert!(signal.duplication_ratio < TEST_SIGNAL_BANDS[0].exit_ratio);
            assert_eq!(signal.transition, None);
            assert_eq!(staggered.signal_band(), 1);
        }

        #[rstest]
        fn band_duration_counts_uncapped_consensus_time_per_band(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            // Runs one cycle: off for `off_ms`, a burst into band 2, one gap of
            // `stall_ms` inside the band, then empty commits until the signal is off.
            let cycle = |off_ms: u64, stall_ms: u64| {
                let entered = staggered.record_txns_after(off_ms, 1000, 2000, 3000);
                assert_eq!(entered.transition, Some(2));
                staggered.record_txns_after(stall_ms, 0, 0, 0);
                let mut in_band_ms = stall_ms;
                for _ in 0..2 * (TEST_SIGNAL_WINDOW_COMMITS + TEST_SIGNAL_EXIT_DWELL_COMMITS) {
                    let signal = staggered.record_txns_after(TEST_COMMIT_INTERVAL_MS, 0, 0, 0);
                    in_band_ms += TEST_COMMIT_INTERVAL_MS;
                    if signal.transition.is_some() {
                        assert_eq!(signal.transition, Some(0));
                        return (
                            entered.ms_in_previous_band,
                            signal.ms_in_previous_band,
                            in_band_ms,
                        );
                    }
                }
                panic!("signal never left band 2");
            };
            // The first cycle starts at the first commit, so the signal spends no time
            // off. The stall is longer than the gap cap and counts in full.
            let stall_ms = 10 * TEST_MAX_COMMIT_GAP_MS;
            let (off_ms, band_ms, expected_band_ms) = cycle(0, stall_ms);
            assert_eq!(off_ms, 0);
            assert_eq!(band_ms, expected_band_ms);
            // The second cycle starts from zero after each transition.
            let (off_ms, band_ms, expected_band_ms) = cycle(7_000, TEST_COMMIT_INTERVAL_MS);
            assert_eq!(off_ms, 7_000);
            assert_eq!(band_ms, expected_band_ms);
        }

        #[test]
        fn time_clock_first_commit_advances_by_zero() {
            let staggered = StaggeredSubmission::new(test_signal_config(TestClock::Time), true);
            staggered.record_commit(1_000_000, 0, 100, 100);
            assert_eq!(staggered.signal.lock().clock, 0);
            staggered.record_commit(1_000_250, 0, 100, 100);
            assert_eq!(staggered.signal.lock().clock, 250);
        }

        #[test]
        fn time_clock_follows_irregular_gaps() {
            let staggered = StaggeredSubmission::new(
                SignalConfig {
                    clock: SignalClock::CommitTimeMs {
                        max_commit_gap_ms: 999,
                    },
                    window: 1_000,
                    ..test_signal_config(TestClock::Time)
                },
                true,
            );
            // Each commit carries a distinct count, so the window total names the
            // commits it holds.
            let totals: Vec<u64> = [
                (0, 1),
                (50, 2),
                (900, 4),
                (1_000, 8),
                (1_000, 16),
                (1_950, 32),
            ]
            .into_iter()
            .map(|(timestamp_ms, txns)| {
                staggered
                    .record_commit(timestamp_ms, 0, txns, txns)
                    .window_consensus_user_txns
            })
            .collect();
            // The commit at 1,000 ms evicts the commit at 0 ms. The commits at 1,000 ms
            // share a timestamp and both stay. The commit at 1,950 ms evicts the
            // commits at 50 and 900 ms.
            assert_eq!(totals, vec![1, 3, 7, 14, 30, 56]);
        }

        #[test]
        fn time_clock_bounds_the_window_when_timestamps_stand_still() {
            let staggered = StaggeredSubmission::new(
                SignalConfig {
                    clock: SignalClock::CommitTimeMs {
                        max_commit_gap_ms: 1,
                    },
                    window: 5,
                    ..test_signal_config(TestClock::Time)
                },
                true,
            );
            for _ in 0..8 {
                staggered.record_commit(1_000, 0, 100, 100);
            }
            let signal = staggered.signal.lock();
            assert_eq!(signal.clock, 0);
            assert_eq!(signal.window.len(), 5);
            assert_eq!(signal.window_sum.consensus_user_txns, 500);
        }

        #[rstest]
        fn signal_state_is_independent_of_manual_activation(
            #[values(TestClock::Commits, TestClock::Time)] clock: TestClock,
        ) {
            let staggered = StaggeredSubmission::new(test_signal_config(clock), true);
            staggered.set_active(true);
            // The signal starts at off, so quiet commits cause no transition and the
            // manual setting stays.
            for _ in 0..50 {
                assert_eq!(staggered.record(0, 100).transition, None);
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

    fn test_quota(
        max_pending_transactions: usize,
        max_pending_bytes: Option<u64>,
        metrics: &AdmissionQueueMetrics,
    ) -> StaggerQuota {
        StaggerQuota::new(max_pending_transactions, max_pending_bytes, 50, metrics)
    }

    /// Returns the held count and bytes, and checks that both gauges match them.
    fn held(quota: &StaggerQuota) -> (usize, u64) {
        let held = quota.held.load(Ordering::Relaxed);
        let held_bytes = quota.held_bytes.load(Ordering::Relaxed);
        assert_eq!(quota.metric.get(), held as i64);
        assert_eq!(quota.bytes_metric.get(), held_bytes as i64);
        (held, held_bytes)
    }

    fn quota_rejections(metrics: &AdmissionQueueMetrics, quota: &str) -> u64 {
        metrics
            .pool_staggered_quota_rejected
            .with_label_values(&[quota])
            .get()
    }

    #[test]
    fn held_byte_quota_bounds_held_bytes() {
        let metrics = AdmissionQueueMetrics::new_for_tests();
        // The quotas are half the capacities: 25 entries and 100 bytes.
        let quota = test_quota(50, Some(200), &metrics);
        let delay = Duration::from_secs(1);

        let first = quota.hold(delay, 60).unwrap();
        let mut second = quota.hold(delay, 60).unwrap();
        assert_eq!(held(&quota), (2, 120));

        // The byte quota is full, so a further hold is refused and leaves both totals
        // unchanged.
        assert!(matches!(
            quota.hold(delay, 1).err().unwrap().as_inner(),
            SuiErrorKind::StaggeredSubmissionHoldQuotaFull
        ));
        assert_eq!(held(&quota), (2, 120));
        assert_eq!(quota_rejections(&metrics, "bytes"), 1);
        assert_eq!(quota_rejections(&metrics, "entries"), 0);

        // Dropping a hold frees its bytes.
        drop(first);
        let third = quota.hold(delay, 60).unwrap();
        assert_eq!(held(&quota), (2, 120));

        // Consumption into a proposal frees the bytes, and a requeue charges them again.
        second.release();
        assert_eq!(held(&quota), (1, 60));
        second.rearm();
        assert_eq!(held(&quota), (2, 120));

        drop(second);
        drop(third);
        assert_eq!(held(&quota), (0, 0));
    }

    #[test]
    fn held_quota_percent_scales_both_quotas() {
        let metrics = AdmissionQueueMetrics::new_for_tests();
        let full = StaggerQuota::new(20_000, Some(1 << 28), 100, &metrics);
        assert_eq!(full.quota, 20_000);
        assert_eq!(full.byte_quota(), Some(1 << 28));
        let quarter = StaggerQuota::new(20_000, Some(1 << 28), 25, &metrics);
        assert_eq!(quarter.quota, 5_000);
        assert_eq!(quarter.byte_quota(), Some(1 << 26));
        // A small capacity still admits one hold.
        let tiny = StaggerQuota::new(1, Some(1), 1, &metrics);
        assert_eq!(tiny.quota, 1);
        assert_eq!(tiny.byte_quota(), Some(1));
    }

    #[test]
    #[should_panic(expected = "between 1 and 100")]
    fn held_quota_percent_above_100_is_rejected() {
        StaggerQuota::new(10, None, 101, &AdmissionQueueMetrics::new_for_tests());
    }

    #[test]
    fn without_a_byte_capacity_only_the_entry_quota_binds() {
        let metrics = AdmissionQueueMetrics::new_for_tests();
        // The entry quota is 4 / 2 = 2 holds.
        let quota = test_quota(4, None, &metrics);
        let delay = Duration::from_secs(1);
        let first = quota.hold(delay, 1 << 30).unwrap();
        let second = quota.hold(delay, 1 << 30).unwrap();
        assert!(matches!(
            quota.hold(delay, 1).err().unwrap().as_inner(),
            SuiErrorKind::StaggeredSubmissionHoldQuotaFull
        ));
        assert_eq!(quota_rejections(&metrics, "entries"), 1);
        assert_eq!(quota_rejections(&metrics, "bytes"), 0);
        drop(first);
        drop(second);
        assert_eq!(held(&quota), (0, 0));
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
    use crate::authority::AuthorityState;
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use crate::consensus_adapter::ConsensusAdapterMetrics;
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
        // Capacity 3 gives a held quota of 75% of 3, rounded down to 2.
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let pool = Arc::new(ConsensusTransactionPool::new_for_tests(
            epoch_store.clone(),
            state.get_cache_writer().clone(),
            3,
            metrics.clone(),
        ));

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
                sui_types::error::SuiErrorKind::StaggeredSubmissionHoldQuotaFull
            ),
            "unexpected quota rejection error: {error}"
        );
        assert_eq!(
            metrics
                .pool_staggered_quota_rejected
                .with_label_values(&["entries"])
                .get(),
            1
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
    async fn expired_held_entries_release_their_quota() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c
        });
        let (state, sender, keypair, _, _, candidates) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let ttl = Duration::from_secs(1);
        let pool = Arc::new(ConsensusTransactionPool::new(
            epoch_store.clone(),
            state.get_cache_writer().clone(),
            10,
            None,
            Some(ttl),
            None,
            // 20% of capacity 10 gives a held quota of 2.
            20,
            metrics.clone(),
            crate::consensus_adapter::ConsensusAdapterMetrics::new_test(),
        ));

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
        let mut receivers = Vec::new();
        for _ in 0..2 {
            let gas = held_gas.next().expect("not enough held-slot gas objects");
            receivers.push(insert(&pool, &state, sender, &keypair, gas).await.0);
        }
        assert_eq!(metrics.pool_staggered_held.get(), 2);

        tokio::time::sleep(ttl + Duration::from_millis(200)).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert!(transactions.is_empty());
        drop(ack);
        assert_eq!(metrics.pool_staggered_held.get(), 0);
        for receiver in receivers {
            assert!(matches!(
                receiver.await.unwrap().unwrap_err().as_inner(),
                SuiErrorKind::ValidatorOverloadedRetryAfter { .. }
            ));
        }

        let gas = held_gas.next().expect("not enough held-slot gas objects");
        let (_receiver, _) = insert(&pool, &state, sender, &keypair, gas).await;
        pool.close();
    }

    #[tokio::test]
    async fn held_byte_quota_leaves_byte_capacity_for_unheld_entries() {
        const MAX_BLOCK_BYTES: u64 = 4096;
        const HELD_QUOTA_PERCENT: u8 = 50;
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut c| {
            c.set_allowed_proposers_for_testing(true);
            c.set_consensus_max_transactions_in_block_bytes_for_testing(MAX_BLOCK_BYTES);
            c
        });
        let (state, sender, keypair, _, immediate_gas, candidates) = setup().await;
        let epoch_store = state.epoch_store_for_testing();
        // The byte capacity is raised to the block limit, so held entries may take half
        // of it. The entry quota of 50% of 100 = 50 is
        // not reached.
        let metrics = Arc::new(AdmissionQueueMetrics::new_for_tests());
        let pool = ConsensusTransactionPool::new(
            epoch_store.clone(),
            state.get_cache_writer().clone(),
            100,
            None,
            None,
            Some(1),
            HELD_QUOTA_PERCENT,
            metrics.clone(),
            ConsensusAdapterMetrics::new_test(),
        );
        let byte_quota = MAX_BLOCK_BYTES * u64::from(HELD_QUOTA_PERCENT) / 100;

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

        // Hold entries until the byte quota refuses one.
        let error = loop {
            let gas = held_gas.next().expect("not enough held-slot gas objects");
            let tx = test_user_transaction(&state, sender, &keypair, gas, vec![]).await;
            let consensus_tx =
                ConsensusTransaction::new_user_transaction_v2_message(&state.name, tx.into());
            if let Err(error) = pool.try_insert(pool.epoch(), 1, vec![consensus_tx]) {
                break error;
            }
        };
        assert!(
            matches!(
                error.as_inner(),
                SuiErrorKind::StaggeredSubmissionHoldQuotaFull
            ),
            "unexpected quota rejection error: {error}"
        );
        let quota_rejections = |quota: &str| {
            metrics
                .pool_staggered_quota_rejected
                .with_label_values(&[quota])
                .get()
        };
        assert_eq!(quota_rejections("bytes"), 1);
        assert_eq!(quota_rejections("entries"), 0);

        // Every queued entry is held, so the bytes charged against the quota are the
        // bytes the lane counts.
        let held_bytes = metrics.pool_staggered_held_bytes.get();
        assert_eq!(
            held_bytes,
            metrics.pool_bytes.with_label_values(&["user"]).get()
        );
        assert!(held_bytes >= byte_quota as i64);

        // An entry that is not held still fits within the byte capacity and is proposed.
        let (_receiver, _) = insert(&pool, &state, sender, &keypair, immediate_gas).await;
        let (transactions, ack, _) = pool.take(10, usize::MAX);
        assert_eq!(transactions.len(), 1, "immediate slot was held");
        drop(ack);
        pool.close();
        assert_eq!(metrics.pool_staggered_held_bytes.get(), 0);
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
