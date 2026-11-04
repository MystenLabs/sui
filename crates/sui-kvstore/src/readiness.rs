// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Decides when this instance has caught up after startup and can report ready.
//!
//! For every pipeline, this instance compares its durable watermark ([`crate::DurableProgress`])
//! with two references, in chain time (`timestamp_ms_hi_inclusive`):
//! - the pipeline's stored watermark row, read once at startup: how far the instances running at
//!   that moment had got, and
//! - the most advanced durable watermark any peer instance has streamed
//!   ([`crate::streamed_watermarks`]).
//!
//! The instance is ready once every pipeline has recorded durable progress since startup and is
//! at most `max_lag_ms` of chain time behind both references.
//!
//! Recording durable progress proves this instance can ingest and write: each pipeline must
//! commit at least once after startup. A pipeline records progress only when it commits new
//! checkpoints, so while no new checkpoints arrive (for example during a chain halt), a restarted
//! instance stays not ready. A rolling deployment then waits on it, and the instances it would
//! replace keep serving.
//!
//! The stored rows read at startup are fixed, so an instance that keeps making progress always
//! catches up to them. Peers keep advancing: a restarted instance catches up faster than the chain
//! advances and closes in on them, but an instance that stays more than `max_lag_ms` behind its
//! peers (for example because its checkpoint source lags theirs) would never match them. Peers
//! can therefore hold up readiness only for a bounded time after the instance has caught up to the
//! stored rows: as long again as that took, and at least [`MIN_PEER_WAIT`]. While catching up to
//! the stored rows, the chain moved on by the time that took; an instance that catches up at least
//! twice as fast as the chain advances closes that gap within the same time again. Once the peer
//! wait is over, the instance reports ready and logs the pipelines still behind its peers.
//!
//! Peers that cannot be reached do not hold up readiness. Before reporting ready, the instance
//! waits for every peer's first response or for its first attempt to subscribe to fail, which is
//! bounded by timeouts, so an instance that catches up to the stored rows quickly still compares
//! itself with its peers.
//!
//! Peers only hold back pipelines near their tip. A pipeline whose peer watermark is more than
//! [`PEER_TIP_WINDOW`] of chain time behind the peers' most advanced watermark across all
//! pipelines is backfilling. Instances backfilling the same pipeline advance through chain time
//! much faster than the chain does, so they rarely stay within `max_lag_ms` of each other; such a
//! pipeline is compared only with its stored row read at startup.
//!
//! Comparisons use chain time, not wall-clock time: if the whole system lags behind the chain tip,
//! instances still become ready as soon as they match each other.
//!
//! Readiness latches: [`wait_until_caught_up`] returns the first time every pipeline is caught
//! up, and later lag does not make the instance unready again.

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::watch;
use tokio::time::Instant;
use tracing::info;
use tracing::warn;

use crate::WatermarkV1;
use crate::streamed_watermarks::StreamedWatermarks;

/// Minimum time peers can hold up readiness after the instance has caught up to the stored
/// watermark rows read at startup.
pub const MIN_PEER_WAIT: Duration = Duration::from_secs(60);

/// How far, in chain time, a pipeline's peer watermark can trail the peers' most advanced
/// watermark across all pipelines before the pipeline counts as backfilling and its peer
/// watermark stops holding up readiness.
pub const PEER_TIP_WINDOW: Duration = Duration::from_secs(60);

/// Minimum time between log lines describing why the instance is not ready yet.
const BLOCKER_LOG_INTERVAL: Duration = Duration::from_secs(30);

#[derive(clap::Args, Clone, Debug)]
pub struct ReadinessArgs {
    /// Largest allowed gap, in milliseconds of chain time, between a pipeline's durable watermark on this instance and both its stored watermark read at startup and the most advanced peer's durable watermark, before the instance reports ready. Requires `--rpc-listen-address`.
    #[arg(
        long,
        default_value_t = Self::default().readiness_max_lag_ms,
        requires = "rpc_listen_address"
    )]
    pub readiness_max_lag_ms: u64,

    /// gRPC URL of a peer kvstore instance (for example `http://kvstore-1-grpc:7001`) whose durable watermarks this instance must catch up to before it reports ready. Repeat for each peer. Requires `--rpc-listen-address`.
    #[arg(long = "peer-url", value_name = "URL", requires = "rpc_listen_address")]
    pub peer_urls: Vec<String>,
}

impl Default for ReadinessArgs {
    fn default() -> Self {
        Self {
            readiness_max_lag_ms: 1_000,
            peer_urls: vec![],
        }
    }
}

/// Why a pipeline keeps the instance from reporting ready.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadinessBlocker {
    /// This process has not recorded durable progress for the pipeline yet.
    NoDurableWatermark,
    /// The stored watermark read at startup is more than the allowed lag ahead of this process's
    /// durable watermark, measured in milliseconds of chain time.
    BehindStartupWatermark { lag_ms: u64 },
    /// The most advanced peer's durable watermark is more than the allowed lag ahead of this
    /// process's durable watermark, measured in milliseconds of chain time.
    BehindPeers { lag_ms: u64 },
}

impl fmt::Display for ReadinessBlocker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDurableWatermark => write!(f, "no durable watermark"),
            Self::BehindStartupWatermark { lag_ms } => {
                write!(f, "{lag_ms}ms behind the stored watermark read at startup")
            }
            Self::BehindPeers { lag_ms } => write!(f, "{lag_ms}ms behind peers"),
        }
    }
}

/// Returns every pipeline in `pipelines` that keeps the instance from being ready, with the
/// reason. An empty result means the instance has caught up.
///
/// `local` holds this process's durable watermarks, `startup` the stored watermark rows read at
/// startup, and `peers` the most advanced durable watermarks peers have reported. A stored row
/// without a checkpoint, or a pipeline missing from `startup` or `peers`, imposes no lag limit,
/// and neither does a peer watermark more than [`PEER_TIP_WINDOW`] behind the peers' tip.
pub(crate) fn readiness_blockers<'p, P: AsRef<str>>(
    pipelines: &'p [P],
    local: &HashMap<String, CommitterWatermark>,
    startup: &HashMap<String, WatermarkV1>,
    peers: &HashMap<String, CommitterWatermark>,
    max_lag_ms: u64,
) -> Vec<(&'p str, ReadinessBlocker)> {
    let peer_tip_ms = peers
        .values()
        .map(|peer| peer.timestamp_ms_hi_inclusive)
        .max()
        .unwrap_or(0);
    let near_peer_tip = |peer: &&CommitterWatermark| {
        peer.timestamp_ms_hi_inclusive
            .saturating_add(PEER_TIP_WINDOW.as_millis() as u64)
            >= peer_tip_ms
    };

    pipelines
        .iter()
        .map(<P as AsRef<str>>::as_ref)
        .filter_map(|pipeline| {
            let Some(local) = local.get(pipeline) else {
                return Some((pipeline, ReadinessBlocker::NoDurableWatermark));
            };
            let lag_behind =
                |timestamp_ms: u64| timestamp_ms.saturating_sub(local.timestamp_ms_hi_inclusive);

            let startup_lag_ms = startup
                .get(pipeline)
                .filter(|stored| stored.checkpoint_hi_inclusive.is_some())
                .map_or(0, |stored| lag_behind(stored.timestamp_ms_hi_inclusive));
            if startup_lag_ms > max_lag_ms {
                return Some((
                    pipeline,
                    ReadinessBlocker::BehindStartupWatermark {
                        lag_ms: startup_lag_ms,
                    },
                ));
            }

            let peer_lag_ms = peers
                .get(pipeline)
                .filter(near_peer_tip)
                .map_or(0, |peer| lag_behind(peer.timestamp_ms_hi_inclusive));
            (peer_lag_ms > max_lag_ms).then_some((
                pipeline,
                ReadinessBlocker::BehindPeers {
                    lag_ms: peer_lag_ms,
                },
            ))
        })
        .collect()
}

/// Resolves once every pipeline in `pipelines` is caught up according to the module-level rule.
///
/// `local` is this process's durable watermarks, `startup` the stored watermark rows read at
/// startup, and `peers` what this instance has heard from its peers. Re-evaluates whenever
/// `local` or `peers` changes, and when the peer wait ends. If the `peers` sender is dropped,
/// the latest peer watermarks still count.
///
/// If the `local` sender is dropped, the store and therefore the indexer have stopped, so the
/// process is shutting down; the future then never resolves and readiness is never reported.
pub async fn wait_until_caught_up(
    pipelines: Vec<String>,
    mut local: watch::Receiver<HashMap<String, CommitterWatermark>>,
    startup: HashMap<String, WatermarkV1>,
    mut peers: watch::Receiver<StreamedWatermarks>,
    max_lag_ms: u64,
) {
    let started = Instant::now();
    let no_peer_watermarks = HashMap::new();
    let mut peers_open = true;
    let mut last_logged: Option<Instant> = None;

    // When peers stop holding up readiness. Set once every pipeline has caught up to `startup`.
    let mut peer_wait_deadline: Option<Instant> = None;

    loop {
        let now = Instant::now();
        let (blockers, unsettled_peers) = {
            let local_snapshot = local.borrow_and_update();
            let peer_snapshot = peers.borrow_and_update();

            if peer_wait_deadline.is_none()
                && readiness_blockers(
                    &pipelines,
                    &local_snapshot,
                    &startup,
                    &no_peer_watermarks,
                    max_lag_ms,
                )
                .is_empty()
            {
                let elapsed = now - started;
                let peer_wait = elapsed.max(MIN_PEER_WAIT);
                peer_wait_deadline = Some(now + peer_wait);
                info!(
                    ?elapsed,
                    ?peer_wait,
                    "Caught up to the stored watermarks read at startup",
                );
            }

            let blockers = readiness_blockers(
                &pipelines,
                &local_snapshot,
                &startup,
                &peer_snapshot.watermarks,
                max_lag_ms,
            );
            (blockers, peer_snapshot.unsettled_instances)
        };

        let peer_wait_over = peer_wait_deadline.is_some_and(|deadline| now >= deadline);
        let (behind_peers, blocking): (Vec<_>, Vec<_>) =
            blockers.into_iter().partition(|(_, blocker)| {
                peer_wait_over && matches!(blocker, ReadinessBlocker::BehindPeers { .. })
            });

        if blocking.is_empty() && (unsettled_peers == 0 || peer_wait_over) {
            if !behind_peers.is_empty() {
                warn!(
                    max_lag_ms,
                    blockers = %format_blockers(&behind_peers),
                    "Peer wait is over; reporting ready while still behind peers",
                );
            }
            info!(
                ?pipelines,
                max_lag_ms, "All pipelines caught up; instance is ready"
            );
            return;
        }

        if last_logged.is_none_or(|logged| logged.elapsed() >= BLOCKER_LOG_INTERVAL) {
            info!(
                max_lag_ms,
                unsettled_peers,
                blockers = %format_blockers(&blocking),
                "Waiting for pipelines to catch up before reporting ready",
            );
            last_logged = Some(now);
        }

        let pending_deadline = peer_wait_deadline.filter(|deadline| now < *deadline);
        tokio::select! {
            changed = local.changed() => {
                if changed.is_err() {
                    std::future::pending::<()>().await;
                }
            }
            changed = peers.changed(), if peers_open => {
                if changed.is_err() {
                    peers_open = false;
                }
            }
            _ = tokio::time::sleep_until(pending_deadline.unwrap_or(now)),
                if pending_deadline.is_some() => {}
        }
    }
}

fn format_blockers(blockers: &[(&str, ReadinessBlocker)]) -> String {
    blockers
        .iter()
        .map(|(pipeline, blocker)| format!("{pipeline}: {blocker}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIPELINE: &str = "pipeline";
    const OTHER_PIPELINE: &str = "other_pipeline";
    const MAX_LAG_MS: u64 = 1_000;

    fn local_watermark(checkpoint: u64, timestamp_ms: u64) -> CommitterWatermark {
        CommitterWatermark {
            checkpoint_hi_inclusive: checkpoint,
            timestamp_ms_hi_inclusive: timestamp_ms,
            ..Default::default()
        }
    }

    fn stored_watermark(checkpoint: Option<u64>, timestamp_ms: u64) -> WatermarkV1 {
        WatermarkV1 {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: checkpoint,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: timestamp_ms,
            reader_lo: 0,
            pruner_hi: 0,
            pruner_timestamp_ms: 0,
            bucket_start_cp: None,
        }
    }

    fn map<W>(entries: impl IntoIterator<Item = (&'static str, W)>) -> HashMap<String, W> {
        entries
            .into_iter()
            .map(|(pipeline, watermark)| (pipeline.to_string(), watermark))
            .collect()
    }

    fn settled_peers(
        entries: impl IntoIterator<Item = (&'static str, CommitterWatermark)>,
    ) -> StreamedWatermarks {
        StreamedWatermarks {
            unsettled_instances: 0,
            watermarks: map(entries),
        }
    }

    /// Asserts that `wait` is still pending after `duration`.
    async fn assert_pending_for(
        wait: &mut (impl Future<Output = ()> + Unpin),
        duration: Duration,
        message: &str,
    ) {
        assert!(
            tokio::time::timeout(duration, wait).await.is_err(),
            "{message}"
        );
    }

    #[test]
    fn no_local_entry_blocks() {
        let startup = map([(PIPELINE, stored_watermark(Some(10), 5_000))]);
        assert_eq!(
            readiness_blockers(
                &[PIPELINE],
                &HashMap::new(),
                &startup,
                &HashMap::new(),
                MAX_LAG_MS
            ),
            vec![(PIPELINE, ReadinessBlocker::NoDurableWatermark)],
        );
    }

    #[test]
    fn no_references_is_ready() {
        let local = map([(PIPELINE, local_watermark(1, 0))]);
        assert!(
            readiness_blockers(
                &[PIPELINE],
                &local,
                &HashMap::new(),
                &HashMap::new(),
                MAX_LAG_MS
            )
            .is_empty()
        );
    }

    #[test]
    fn startup_row_without_checkpoint_is_ready() {
        let local = map([(PIPELINE, local_watermark(1, 0))]);
        let startup = map([(PIPELINE, stored_watermark(None, u64::MAX))]);
        assert!(
            readiness_blockers(&[PIPELINE], &local, &startup, &HashMap::new(), MAX_LAG_MS)
                .is_empty()
        );
    }

    #[test]
    fn lag_equal_to_max_is_ready() {
        let local = map([(PIPELINE, local_watermark(1, 10_000))]);
        let startup = map([(PIPELINE, stored_watermark(Some(5), 10_000 + MAX_LAG_MS))]);
        let peers = map([(PIPELINE, local_watermark(5, 10_000 + MAX_LAG_MS))]);
        assert!(readiness_blockers(&[PIPELINE], &local, &startup, &peers, MAX_LAG_MS).is_empty());
    }

    #[test]
    fn behind_startup_row_blocks_with_lag() {
        let local = map([(PIPELINE, local_watermark(1, 10_000))]);
        let startup = map([(PIPELINE, stored_watermark(Some(5), 10_000 + MAX_LAG_MS + 1))]);
        assert_eq!(
            readiness_blockers(&[PIPELINE], &local, &startup, &HashMap::new(), MAX_LAG_MS),
            vec![(
                PIPELINE,
                ReadinessBlocker::BehindStartupWatermark {
                    lag_ms: MAX_LAG_MS + 1
                }
            )],
        );
    }

    #[test]
    fn behind_peers_blocks_with_lag() {
        let local = map([(PIPELINE, local_watermark(10, 50_000))]);
        let startup = map([(PIPELINE, stored_watermark(Some(10), 50_000))]);
        let peers = map([(PIPELINE, local_watermark(20, 50_000 + MAX_LAG_MS + 1))]);
        assert_eq!(
            readiness_blockers(&[PIPELINE], &local, &startup, &peers, MAX_LAG_MS),
            vec![(
                PIPELINE,
                ReadinessBlocker::BehindPeers {
                    lag_ms: MAX_LAG_MS + 1
                }
            )],
        );
    }

    #[test]
    fn only_blocked_pipeline_is_reported() {
        let local = map([
            (PIPELINE, local_watermark(10, 50_000)),
            (OTHER_PIPELINE, local_watermark(1, 0)),
        ]);
        let startup = map([
            (PIPELINE, stored_watermark(Some(10), 50_000)),
            (OTHER_PIPELINE, stored_watermark(Some(10), 50_000)),
        ]);
        assert_eq!(
            readiness_blockers(
                &[PIPELINE, OTHER_PIPELINE],
                &local,
                &startup,
                &HashMap::new(),
                MAX_LAG_MS
            ),
            vec![(
                OTHER_PIPELINE,
                ReadinessBlocker::BehindStartupWatermark { lag_ms: 50_000 }
            )],
        );
    }

    #[test]
    fn peers_hold_back_only_pipelines_near_their_tip() {
        let tip_ms = 100_000_000;
        let window_ms = PEER_TIP_WINDOW.as_millis() as u64;
        let backfill_ms = tip_ms - window_ms - 30_000;

        // OTHER_PIPELINE is backfilling: this instance is 30s of chain time behind its peers on
        // it, but caught up to its stored row.
        let local = map([
            (PIPELINE, local_watermark(100, tip_ms)),
            (OTHER_PIPELINE, local_watermark(1, backfill_ms)),
        ]);
        let startup = map([
            (PIPELINE, stored_watermark(Some(100), tip_ms)),
            (OTHER_PIPELINE, stored_watermark(Some(1), backfill_ms)),
        ]);
        let backfilling_peers = map([
            (PIPELINE, local_watermark(100, tip_ms)),
            (OTHER_PIPELINE, local_watermark(2, tip_ms - window_ms - 1)),
        ]);
        assert!(
            readiness_blockers(
                &[PIPELINE, OTHER_PIPELINE],
                &local,
                &startup,
                &backfilling_peers,
                MAX_LAG_MS
            )
            .is_empty()
        );

        // Within the window of the peers' tip, the pipeline is held back again.
        let near_tip_peers = map([
            (PIPELINE, local_watermark(100, tip_ms)),
            (OTHER_PIPELINE, local_watermark(2, tip_ms - window_ms)),
        ]);
        assert_eq!(
            readiness_blockers(
                &[PIPELINE, OTHER_PIPELINE],
                &local,
                &startup,
                &near_tip_peers,
                MAX_LAG_MS
            ),
            vec![(
                OTHER_PIPELINE,
                ReadinessBlocker::BehindPeers { lag_ms: 30_000 }
            )],
        );
    }

    #[tokio::test]
    async fn wait_resolves_after_local_catches_up_without_peers() {
        let (local_tx, local_rx) = watch::channel(map([(PIPELINE, local_watermark(1, 0))]));
        let startup = map([(PIPELINE, stored_watermark(Some(10), 50_000))]);
        // No peers configured: nothing ever sends on the peer channel.
        let (_, peers_rx) = watch::channel(StreamedWatermarks::default());

        let wait = wait_until_caught_up(
            vec![PIPELINE.to_string()],
            local_rx,
            startup,
            peers_rx,
            MAX_LAG_MS,
        );
        tokio::pin!(wait);

        assert_pending_for(
            &mut wait,
            Duration::from_millis(100),
            "readiness resolved while behind the stored watermark",
        )
        .await;

        local_tx.send_replace(map([(PIPELINE, local_watermark(10, 50_000))]));

        tokio::time::timeout(Duration::from_secs(5), wait)
            .await
            .expect("readiness did not resolve after catching up to the stored watermark");
    }

    #[tokio::test]
    async fn wait_waits_for_unsettled_peers() {
        let (_local_tx, local_rx) = watch::channel(map([(PIPELINE, local_watermark(10, 50_000))]));
        let (peers_tx, peers_rx) = watch::channel(StreamedWatermarks {
            unsettled_instances: 1,
            watermarks: HashMap::new(),
        });

        let wait = wait_until_caught_up(
            vec![PIPELINE.to_string()],
            local_rx,
            HashMap::new(),
            peers_rx,
            MAX_LAG_MS,
        );
        tokio::pin!(wait);

        assert_pending_for(
            &mut wait,
            Duration::from_millis(100),
            "readiness resolved before hearing from every peer",
        )
        .await;

        peers_tx.send_replace(settled_peers([]));

        tokio::time::timeout(Duration::from_secs(5), wait)
            .await
            .expect("readiness did not resolve after every peer settled");
    }

    #[tokio::test]
    async fn wait_follows_advancing_peers() {
        let (local_tx, local_rx) = watch::channel(map([(PIPELINE, local_watermark(10, 50_000))]));
        let (peers_tx, peers_rx) =
            watch::channel(settled_peers([(PIPELINE, local_watermark(20, 60_000))]));

        let wait = wait_until_caught_up(
            vec![PIPELINE.to_string()],
            local_rx,
            HashMap::new(),
            peers_rx,
            MAX_LAG_MS,
        );
        tokio::pin!(wait);

        assert_pending_for(
            &mut wait,
            Duration::from_millis(100),
            "readiness resolved while behind peers",
        )
        .await;

        // The peer advances as this instance reaches where the peer was.
        peers_tx.send_replace(settled_peers([(PIPELINE, local_watermark(30, 70_000))]));
        local_tx.send_replace(map([(PIPELINE, local_watermark(20, 60_000))]));

        assert_pending_for(
            &mut wait,
            Duration::from_millis(100),
            "readiness resolved against a peer watermark that had since advanced",
        )
        .await;

        local_tx.send_replace(map([(PIPELINE, local_watermark(30, 70_000))]));

        tokio::time::timeout(Duration::from_secs(5), wait)
            .await
            .expect("readiness did not resolve after catching up to peers");
    }

    /// Catches up to the stored watermark `catch_up` after the wait starts while a peer stays
    /// ahead, and asserts the wait resolves exactly `expected_peer_wait` later.
    async fn assert_peer_wait(catch_up: Duration, expected_peer_wait: Duration) {
        let (local_tx, local_rx) = watch::channel(map([(PIPELINE, local_watermark(1, 0))]));
        let startup = map([(PIPELINE, stored_watermark(Some(10), 50_000))]);
        let (_peers_tx, peers_rx) =
            watch::channel(settled_peers([(PIPELINE, local_watermark(1_000, 900_000))]));

        let wait = wait_until_caught_up(
            vec![PIPELINE.to_string()],
            local_rx,
            startup,
            peers_rx,
            MAX_LAG_MS,
        );
        tokio::pin!(wait);

        assert_pending_for(
            &mut wait,
            catch_up,
            "readiness resolved behind the stored watermark",
        )
        .await;

        local_tx.send_replace(map([(PIPELINE, local_watermark(10, 50_000))]));

        assert_pending_for(
            &mut wait,
            expected_peer_wait - Duration::from_millis(1),
            "readiness resolved before the peer wait was over",
        )
        .await;

        tokio::time::timeout(Duration::from_millis(2), wait)
            .await
            .expect("readiness did not resolve when the peer wait was over");
    }

    #[tokio::test(start_paused = true)]
    async fn peer_wait_lasts_at_least_the_minimum() {
        assert_peer_wait(Duration::from_secs(10), MIN_PEER_WAIT).await;
    }

    #[tokio::test(start_paused = true)]
    async fn peer_wait_lasts_as_long_as_catching_up_to_stored_watermarks() {
        let catch_up = MIN_PEER_WAIT * 3;
        assert_peer_wait(catch_up, catch_up).await;
    }

    #[tokio::test]
    async fn wait_resolves_immediately_without_pipelines() {
        let (_local_tx, local_rx) = watch::channel(HashMap::new());
        let (_peers_tx, peers_rx) = watch::channel(StreamedWatermarks::default());

        tokio::time::timeout(
            Duration::from_secs(5),
            wait_until_caught_up(vec![], local_rx, HashMap::new(), peers_rx, MAX_LAG_MS),
        )
        .await
        .expect("readiness with no pipelines did not resolve immediately");
    }
}
