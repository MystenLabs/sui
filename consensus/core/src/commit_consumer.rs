// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt, sync::Arc};

use consensus_config::Stake;
use consensus_types::block::Round;
use mysten_metrics::monitored_mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::{SetOnce, watch};
use tracing::debug;

use crate::{CommitIndex, CommittedSubDag};

/// Arguments from commit consumer to this consensus instance.
#[derive(Clone)]
pub struct CommitConsumerArgs {
    pub(crate) replay_mode: ReplayMode,
    pub(crate) commit_sender: UnboundedSender<CommittedSubDag>,
    monitor: Arc<CommitConsumerMonitor>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ReplayMode {
    /// Replay from a consumer-supplied watermark without acknowledgement pacing.
    FromWatermark {
        /// Commit indices start at one. Replay starts at this index plus one;
        /// zero requests the entire epoch.
        replay_after_commit_index: CommitIndex,
        /// Consumer-supplied target for startup readiness.
        consumer_last_processed_commit_index: CommitIndex,
    },
    /// Replay the entire epoch, discovering the target from consensus storage.
    /// Only the finalized prefix is paced by application acknowledgements;
    /// the unfinalized tail and live consensus keep their existing flow control.
    FullEpochPaced,
}

impl CommitConsumerArgs {
    /// Requests replay after `replay_after_commit_index`, using the consumer's
    /// last processed index as the recovery target. This mode does not wait for
    /// application acknowledgements during startup.
    /// Commit indices start at one; use zero to replay from the first commit.
    pub fn new(
        replay_after_commit_index: CommitIndex,
        consumer_last_processed_commit_index: CommitIndex,
    ) -> (Self, UnboundedReceiver<CommittedSubDag>) {
        Self::new_with_replay_mode(ReplayMode::FromWatermark {
            replay_after_commit_index,
            consumer_last_processed_commit_index,
        })
    }

    /// Replays the epoch from its first commit into a consumer with no durable
    /// execution watermark. Consensus owns storage and delivers historical and
    /// live commits over the returned stream.
    ///
    /// This is an integration API for external consensus consumers that rebuild
    /// application state on restart. DO NOT REMOVE this entry point solely because it
    /// has no production callers in this workspace. Changes to this contract
    /// must account for downstream consumers.
    ///
    /// Consensus discovers the startup target from its store: the last stored
    /// commit when transaction voting is disabled, or the last finalized commit
    /// when voting is enabled. It waits for application acknowledgements between
    /// recovery batches. An unfinalized tail goes through normal finalization
    /// without waiting for application at startup, since it may need live consensus.
    /// Consequently, pacing bounds only the finalized prefix: if voting is enabled
    /// and no commits are finalized, the entire history is queued without pacing.
    ///
    /// The consumer must run before awaiting [`crate::ConsensusAuthority::start`]
    /// and call [`CommitConsumerMonitor::set_highest_handled_commit`] only after
    /// applying each commit. Waiting for startup before consuming would deadlock
    /// recovery. Use [`Self::monitor`] to observe readiness and committed-head
    /// progress without accessing storage.
    /// Startup panics if the consumer closes while recovery is waiting for an
    /// application acknowledgement.
    pub fn new_with_full_replay() -> (Self, UnboundedReceiver<CommittedSubDag>) {
        Self::new_with_replay_mode(ReplayMode::FullEpochPaced)
    }

    fn new_with_replay_mode(replay_mode: ReplayMode) -> (Self, UnboundedReceiver<CommittedSubDag>) {
        let (replay_after_commit_index, replay_target) = match replay_mode {
            ReplayMode::FromWatermark {
                replay_after_commit_index,
                consumer_last_processed_commit_index,
            } => (
                replay_after_commit_index,
                Some(consumer_last_processed_commit_index),
            ),
            ReplayMode::FullEpochPaced => (0, None),
        };
        let (commit_sender, commit_receiver) = unbounded_channel("consensus_commit_output");
        let monitor = Arc::new(CommitConsumerMonitor::new(
            replay_after_commit_index,
            replay_target,
        ));
        (
            Self {
                replay_mode,
                commit_sender,
                monitor,
            },
            commit_receiver,
        )
    }

    pub fn monitor(&self) -> Arc<CommitConsumerMonitor> {
        self.monitor.clone()
    }
}

/// Process-local committed head reported by consensus, independently of application progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitProgress {
    /// Latest committed index reported by consensus, independently of consumer progress.
    /// Recovery reports the stored head before replay, rather than each historical
    /// commit as it is delivered. Live commits advance this value afterwards.
    pub highest_committed_index: CommitIndex,
    /// Leader round associated with [`Self::highest_committed_index`], including
    /// the stored head during replay rather than the consumer's replay position.
    pub highest_committed_round: Round,
}

/// This node's view of committee progress, assembled from data consensus already gathers.
/// The round prober refreshes the round and stake fields after every complete probe. The commit
/// syncer checks its quorum commit index on each scheduling pass and refreshes the commit index
/// when it rose.
///
/// When local commits stop, embedders can use these observations to investigate:
/// - Before the first complete probe, `probed_at_ms`, `responsive_stake`, and
///   `quorum_accepted_round` are zero. The commit syncer publishes `quorum_commit_index`
///   independently and can report progress before that probe.
/// - After a complete probe, zero responsive stake means no remote peer answered successfully.
///   Local isolation and an outage of all other validators give the same result, even over
///   repeated probes. A watchdog needs independent evidence before attributing this to a local
///   fault. At epoch startup, peers may also be starting or still serving the previous epoch.
/// - If this validator is participating, compare `own_stake + responsive_stake` with the
///   committee's quorum threshold to check whether this validator and its responding peers
///   represent quorum stake. Peers may answer probes while consensus is stalled.
///   In a single-validator committee, responsive stake is always zero.
/// - If the reachable stake falls below quorum, the rest of the committee may still be live.
///   For example, when seven validators with equal stake split into groups of two and five,
///   a validator in the group of two sees one responsive peer while the group of five can commit.
/// - If the observed quorum commit index keeps advancing beyond this node's committed index
///   in [`CommitProgress`], the node is falling behind the committee.
/// - The round prober runs only on validator nodes. On observer and other non-validator nodes,
///   the round and stake fields keep their defaults and only `quorum_commit_index` updates,
///   from the commit syncer. It carries no freshness stamp, so on those nodes this type cannot
///   separate isolation from a halt.
/// - The monitor belongs to one consensus instance. After that instance stops, the value stops
///   updating and subscribers are not notified. Readers must discard the monitor together with
///   the instance.
///
/// The values are only as accurate as the prober and syncer computations behind them. They
/// are read-only and are not inputs to consensus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PeerProgress {
    /// Highest accepted quorum round observed by this monitor. Each complete probe samples the
    /// maximum of the per-authority `round_tracker_low_accepted_quorum_round` gauges. The
    /// monitor retains the highest sample, even if the gauges later fall. Blocks received
    /// between probes can advance the next sample even when no peer answers that probe.
    pub quorum_accepted_round: Round,
    /// Highest quorum commit index this monitor has observed. The commit syncer publishes the
    /// same sample to its `commit_sync_quorum_index` gauge. The syncer can miss the local
    /// validator's own votes, and applying fetched commits can delay its next scheduling pass.
    /// The committee can keep committing while this index stays flat.
    pub quorum_commit_index: CommitIndex,
    /// Total stake of remote peers whose request succeeded in the last complete probe, including
    /// peers whose response payload was subsequently rejected as malformed.
    pub responsive_stake: Stake,
    /// Time in milliseconds when the last complete probe finished, from the consensus clock.
    /// That clock reads the wall clock once at start and then advances with a monotonic clock,
    /// so it can differ from the system clock after a clock step. Judge freshness by comparing
    /// consecutive values. It is zero before the first probe.
    pub probed_at_ms: u64,
}

/// Receives [`PeerProgress`] updates from a [`CommitConsumerMonitor`].
///
/// Reading and debug formatting both copy the snapshot before handing it to the caller or
/// formatter, so neither holds the watch channel's lock while processing the value.
#[derive(Clone)]
pub struct PeerProgressReceiver {
    receiver: watch::Receiver<PeerProgress>,
}

impl fmt::Debug for PeerProgressReceiver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let latest = *self.receiver.borrow();
        f.debug_struct("PeerProgressReceiver")
            .field("latest", &latest)
            .finish()
    }
}

impl PeerProgressReceiver {
    /// Returns the latest value and marks it as seen.
    pub fn latest(&mut self) -> PeerProgress {
        *self.receiver.borrow_and_update()
    }

    /// Returns true if a value that this receiver has not seen is available. Returns an error
    /// if the monitor has been dropped.
    pub fn has_changed(&self) -> Result<bool, watch::error::RecvError> {
        self.receiver.has_changed()
    }

    /// Waits for a value that this receiver has not seen, marks it as seen and returns it.
    /// After the monitor is dropped, returns the last value if it is still unseen, then an error.
    pub async fn changed(&mut self) -> Result<PeerProgress, watch::error::RecvError> {
        self.receiver.changed().await?;
        Ok(self.latest())
    }
}

/// Tracks application progress, startup readiness, the consensus head, and peer progress.
///
/// External consumers observe the committed head, handled commits, and replay
/// target separately to monitor catch-up without reading consensus storage. Liveness
/// watchdogs use [`PeerProgress`] to interpret a pause in local commits.
/// DO NOT REMOVE these observation APIs solely because they have no callers in
/// this workspace. They support the external integration described by
/// [`CommitConsumerArgs::new_with_full_replay`].
pub struct CommitConsumerMonitor {
    replay_target: SetOnce<CommitIndex>,
    committed: watch::Sender<CommitProgress>,
    handled: watch::Sender<CommitIndex>,
    peer_progress: watch::Sender<PeerProgress>,
}

impl CommitConsumerMonitor {
    pub(crate) fn new(
        replay_after_commit_index: CommitIndex,
        replay_target: Option<CommitIndex>,
    ) -> Self {
        Self {
            replay_target: SetOnce::new_with(replay_target),
            committed: watch::Sender::new(CommitProgress {
                highest_committed_index: 0,
                highest_committed_round: 0,
            }),
            handled: watch::Sender::new(replay_after_commit_index),
            peer_progress: watch::Sender::new(PeerProgress::default()),
        }
    }

    /// Returns the consensus head, independently of consumer acknowledgements.
    pub fn progress(&self) -> CommitProgress {
        *self.committed.borrow()
    }

    /// Subscribes to consensus head changes only. To observe application progress
    /// and target discovery, also use [`Self::subscribe_handled_commit`] and
    /// [`Self::wait_for_replay_target`].
    pub fn subscribe_progress(&self) -> watch::Receiver<CommitProgress> {
        self.committed.subscribe()
    }

    /// Returns the fixed startup target. With [`CommitConsumerArgs::new_with_full_replay`],
    /// `None` means storage has not been inspected and `Some(0)` means there is no
    /// finalized startup work. With [`CommitConsumerArgs::new`], the consumer-supplied
    /// last processed index is available immediately.
    pub fn replay_target(&self) -> Option<CommitIndex> {
        self.replay_target.get().copied()
    }

    /// Waits for target discovery, without waiting for the consumer to apply it.
    pub async fn wait_for_replay_target(&self) -> CommitIndex {
        *self.replay_target.wait().await
    }

    pub(crate) fn set_replay_target(&self, target: CommitIndex) {
        self.replay_target
            .set(target)
            .expect("startup replay target must be set only once");
    }

    pub(crate) fn report_committed(&self, index: CommitIndex, round: Round) {
        self.committed.send_if_modified(|progress| {
            if index <= progress.highest_committed_index {
                return false;
            }
            progress.highest_committed_index = index;
            progress.highest_committed_round = round;
            true
        });
    }

    pub fn highest_handled_commit(&self) -> CommitIndex {
        *self.handled.borrow()
    }

    /// Subscribes to consumer acknowledgements, independently of consensus progress.
    pub fn subscribe_handled_commit(&self) -> watch::Receiver<CommitIndex> {
        self.handled.subscribe()
    }

    /// Acknowledge application, after the consumer has finished processing the commit.
    pub fn set_highest_handled_commit(&self, highest_handled_commit: CommitIndex) {
        debug!("Highest handled commit set to {}", highest_handled_commit);
        self.handled.send_replace(highest_handled_commit);
    }

    /// Returns this node's latest view of committee progress. See [`PeerProgress`].
    pub fn peer_progress(&self) -> PeerProgress {
        *self.peer_progress.borrow()
    }

    /// Subscribes to committee progress. The round prober sends one update per complete probe,
    /// and the commit syncer sends one when a scheduling pass finds a higher quorum commit index.
    pub fn subscribe_peer_progress(&self) -> PeerProgressReceiver {
        PeerProgressReceiver {
            receiver: self.peer_progress.subscribe(),
        }
    }

    /// Records the outcome of one round-prober pass. The quorum round is kept monotonic, because
    /// a probe where peers fail to answer carries no evidence that the committee moved backwards.
    pub(crate) fn report_probe(
        &self,
        quorum_accepted_round: Round,
        responsive_stake: Stake,
        probed_at_ms: u64,
    ) {
        self.peer_progress.send_modify(|progress| {
            progress.quorum_accepted_round =
                progress.quorum_accepted_round.max(quorum_accepted_round);
            progress.responsive_stake = responsive_stake;
            progress.probed_at_ms = probed_at_ms;
        });
    }

    /// Records the quorum commit index observed by the commit syncer. Subscribers are notified
    /// only when the index rises above the highest value already observed.
    pub(crate) fn report_quorum_commit_index(&self, quorum_commit_index: CommitIndex) {
        self.peer_progress.send_if_modified(|progress| {
            if quorum_commit_index <= progress.quorum_commit_index {
                return false;
            }
            progress.quorum_commit_index = quorum_commit_index;
            true
        });
    }

    pub(crate) async fn wait_for_handled(&self, target: CommitIndex) {
        let mut handled = self.subscribe_handled_commit();
        loop {
            if *handled.borrow_and_update() >= target {
                return;
            }
            handled.changed().await.unwrap();
        }
    }

    /// Waits until the consumer has applied the startup replay target, including
    /// target discovery for full replay. An undiscovered target is not an empty
    /// database.
    pub async fn replay_to_consumer_last_processed_commit_complete(&self) {
        let target = self.wait_for_replay_target().await;
        self.wait_for_handled(target).await;
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use rstest::rstest;
    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn test_commit_consumer_monitor() {
        let monitor = CommitConsumerMonitor::new(0, Some(10));
        assert_eq!(monitor.highest_handled_commit(), 0);
        monitor.set_highest_handled_commit(100);
        assert_eq!(monitor.highest_handled_commit(), 100);
        monitor
            .replay_to_consumer_last_processed_commit_complete()
            .await;
    }

    #[tokio::test]
    async fn full_replay_waits_for_storage_and_for_the_consumer() {
        let (args, _receiver) = CommitConsumerArgs::new_with_full_replay();
        let monitor = args.monitor();
        assert!(
            timeout(
                Duration::from_millis(20),
                monitor.replay_to_consumer_last_processed_commit_complete()
            )
            .await
            .is_err()
        );
        monitor.set_replay_target(5);
        monitor.set_highest_handled_commit(4);
        assert!(
            timeout(
                Duration::from_millis(20),
                monitor.replay_to_consumer_last_processed_commit_complete()
            )
            .await
            .is_err()
        );
        monitor.set_highest_handled_commit(5);
        timeout(
            Duration::from_secs(1),
            monitor.replay_to_consumer_last_processed_commit_complete(),
        )
        .await
        .unwrap();
    }

    #[rstest]
    #[case(0)]
    #[case(5)]
    #[tokio::test]
    async fn acknowledged_full_replay_waits_for_target_discovery(#[case] target: CommitIndex) {
        let (args, _receiver) = CommitConsumerArgs::new_with_full_replay();
        let monitor = args.monitor();
        assert_eq!(monitor.replay_target(), None);
        let ready = monitor.replay_to_consumer_last_processed_commit_complete();
        tokio::pin!(ready);
        assert!(futures::poll!(&mut ready).is_pending());

        monitor.set_highest_handled_commit(target);
        assert!(futures::poll!(&mut ready).is_pending());
        monitor.set_replay_target(target);
        timeout(Duration::from_secs(1), ready).await.unwrap();
    }

    #[rstest]
    #[case::discovered_empty(true, 0)]
    #[case::discovered_nonempty(true, 5)]
    #[case::supplied_empty(false, 0)]
    #[case::supplied_nonempty(false, 5)]
    #[tokio::test]
    #[should_panic(expected = "startup replay target must be set only once")]
    async fn replay_target_cannot_be_set_twice(
        #[case] full_replay: bool,
        #[case] target: CommitIndex,
    ) {
        let (args, _receiver) = if full_replay {
            CommitConsumerArgs::new_with_full_replay()
        } else {
            CommitConsumerArgs::new(0, target)
        };
        let monitor = args.monitor();
        if full_replay {
            monitor.set_replay_target(target);
        }
        assert_eq!(monitor.replay_target(), Some(target));
        assert_eq!(monitor.wait_for_replay_target().await, target);
        monitor.set_replay_target(target);
    }

    #[tokio::test]
    async fn committed_handled_and_target_notifications_are_independent() {
        let (args, _receiver) = CommitConsumerArgs::new_with_full_replay();
        let monitor = args.monitor();
        let mut committed = monitor.subscribe_progress();
        let mut handled = monitor.subscribe_handled_commit();
        let target = monitor.wait_for_replay_target();
        tokio::pin!(target);
        assert!(futures::poll!(&mut target).is_pending());

        monitor.report_committed(20, 50);
        timeout(Duration::from_secs(1), committed.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            *committed.borrow_and_update(),
            CommitProgress {
                highest_committed_index: 20,
                highest_committed_round: 50,
            }
        );
        assert!(!handled.has_changed().unwrap());
        assert!(futures::poll!(&mut target).is_pending());

        monitor.set_highest_handled_commit(2);
        timeout(Duration::from_secs(1), handled.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(*handled.borrow_and_update(), 2);
        assert!(!committed.has_changed().unwrap());
        assert!(futures::poll!(&mut target).is_pending());

        monitor.set_replay_target(5);
        assert_eq!(timeout(Duration::from_secs(1), target).await.unwrap(), 5);
        assert!(!committed.has_changed().unwrap());
        assert!(!handled.has_changed().unwrap());
        assert_eq!(monitor.replay_target(), Some(5));
        assert_eq!(monitor.highest_handled_commit(), 2);

        monitor.report_committed(19, 49);
        monitor.report_committed(20, 50);
        assert!(!committed.has_changed().unwrap());
        assert_eq!(monitor.progress(), *committed.borrow());
    }

    #[tokio::test]
    async fn committed_head_advances_independently_of_a_stalled_consumer() {
        let monitor = CommitConsumerMonitor::new(0, Some(0));
        monitor.report_committed(20, 50);
        monitor.set_highest_handled_commit(2);
        monitor.report_committed(21, 52);
        monitor.report_committed(19, 49);
        assert_eq!(monitor.highest_handled_commit(), 2);
        assert_eq!(monitor.progress().highest_committed_index, 21);
        assert_eq!(monitor.progress().highest_committed_round, 52);
    }

    #[tokio::test]
    async fn peer_progress_starts_without_information() {
        let monitor = CommitConsumerMonitor::new(0, Some(0));
        assert_eq!(monitor.peer_progress(), PeerProgress::default());
        let mut receiver = monitor.subscribe_peer_progress();
        assert_eq!(receiver.latest(), PeerProgress::default());

        monitor.report_quorum_commit_index(12);
        assert!(receiver.has_changed().unwrap());
        assert_eq!(
            receiver.latest(),
            PeerProgress {
                quorum_commit_index: 12,
                ..PeerProgress::default()
            }
        );
    }

    #[tokio::test]
    async fn peer_progress_reports_probes_and_quorum_commit_index() {
        let monitor = CommitConsumerMonitor::new(0, Some(0));
        let mut peer_progress = monitor.subscribe_peer_progress();
        let committed = monitor.subscribe_progress();
        let handled = monitor.subscribe_handled_commit();

        monitor.report_probe(30, 7_500, 1_000);
        timeout(Duration::from_secs(1), peer_progress.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            peer_progress.latest(),
            PeerProgress {
                quorum_accepted_round: 30,
                quorum_commit_index: 0,
                responsive_stake: 7_500,
                probed_at_ms: 1_000,
            }
        );

        monitor.report_quorum_commit_index(12);
        let updated = timeout(Duration::from_secs(1), peer_progress.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.quorum_commit_index, 12);

        // Repeating or lowering the quorum commit index is not an update.
        monitor.report_quorum_commit_index(12);
        monitor.report_quorum_commit_index(3);
        assert!(!peer_progress.has_changed().unwrap());
        assert_eq!(monitor.peer_progress().quorum_commit_index, 12);

        // A probe that no peer answered keeps the quorum round and refreshes stake and time.
        monitor.report_probe(0, 0, 2_000);
        timeout(Duration::from_secs(1), peer_progress.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            peer_progress.latest(),
            PeerProgress {
                quorum_accepted_round: 30,
                quorum_commit_index: 12,
                responsive_stake: 0,
                probed_at_ms: 2_000,
            }
        );
        assert_eq!(monitor.peer_progress(), peer_progress.latest());

        // Peer progress does not notify the committed head or handled commit subscribers.
        assert!(!committed.has_changed().unwrap());
        assert!(!handled.has_changed().unwrap());
    }
}
