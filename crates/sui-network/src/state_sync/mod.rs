// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Peer-to-peer data synchronization of checkpoints.
//!
//! This StateSync module is responsible for the synchronization and dissemination of checkpoints
//! and the transactions, and their effects, contained within. This module is *not* responsible for
//! the execution of the transactions included in a checkpoint, that process is left to another
//! component in the system.
//!
//! # High-level Overview of StateSync
//!
//! StateSync discovers new checkpoints via a few different sources:
//! 1. If this node is a Validator, checkpoints will be produced via consensus at which point
//!    consensus can notify state-sync of the new checkpoint via [Handle::send_checkpoint].
//! 2. A peer notifies us of the latest checkpoint which they have synchronized. State-Sync will
//!    also periodically query its peers to discover what their latest checkpoint is.
//!
//! We keep track of two different watermarks:
//! * highest_verified_checkpoint - This is the highest checkpoint header that we've locally
//!   verified. This indicated that we have in our persistent store (and have verified) all
//!   checkpoint headers up to and including this value.
//! * highest_synced_checkpoint - This is the highest checkpoint that we've fully synchronized,
//!   meaning we've downloaded and have in our persistent stores all of the transactions, and their
//!   effects (but not the objects), for all checkpoints up to and including this point. This is
//!   the watermark that is shared with other peers, either via notification or when they query for
//!   our latest checkpoint, and is intended to be used as a guarantee of data availability.
//!
//! The `PeerHeights` struct is used to track the highest_synced_checkpoint watermark for all of
//! our peers.
//!
//! When a new checkpoint is discovered, and we've determined that it is higher than our
//! highest_verified_checkpoint, then StateSync will kick off a task to synchronize and verify all
//! checkpoints between our highest_synced_checkpoint and the newly discovered checkpoint. This
//! process is done by querying one of our peers for the checkpoints we're missing (using the
//! `PeerHeights` struct as a way to intelligently select which peers have the data available for
//! us to query) at which point we will locally verify the signatures on the checkpoint header with
//! the appropriate committee (based on the epoch). As checkpoints are verified, the
//! highest_synced_checkpoint watermark will be ratcheted up.
//!
//! Once we've ratcheted up our highest_verified_checkpoint, and if it is higher than
//! highest_synced_checkpoint, StateSync will then kick off a task to synchronize the contents of
//! all of the checkpoints from highest_synced_checkpoint..=highest_verified_checkpoint. After the
//! contents of each checkpoint is fully downloaded, StateSync will update our
//! highest_synced_checkpoint watermark and send out a notification on a broadcast channel
//! indicating that a new checkpoint has been fully downloaded. Notifications on this broadcast
//! channel will always be made in order. StateSync will also send out a notification to its peers
//! of the newly synchronized checkpoint so that it can help other peers synchronize.

use anemo::{types::PeerEvent, PeerId, Request, Response, Result};
use futures::{stream::FuturesOrdered, FutureExt, StreamExt};
use rand::Rng;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, RwLock},
    time::Duration,
};
use sui_config::p2p::StateSyncConfig;
use sui_types::{
    committee::Committee,
    digests::CheckpointDigest,
    messages_checkpoint::{
        CertifiedCheckpointSummary as Checkpoint, CheckpointSequenceNumber, EndOfEpochData,
        FullCheckpointContents, VerifiedCheckpoint, VerifiedCheckpointContents,
    },
    storage::WriteStore,
};
use tap::{Pipe, TapFallible, TapOptional};
use tokio::sync::oneshot;
use tokio::{
    sync::{broadcast, mpsc, watch},
    task::{AbortHandle, JoinSet},
};
use tracing::{debug, info, instrument, trace, warn};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/sui.StateSync.rs"));
}
mod builder;
mod metrics;
mod server;
#[cfg(test)]
mod tests;

pub use builder::{Builder, UnstartedStateSync};
pub use generated::{
    state_sync_client::StateSyncClient,
    state_sync_server::{StateSync, StateSyncServer},
};
pub use server::GetCheckpointAvailabilityResponse;
pub use server::GetCheckpointSummaryRequest;
use sui_archival::reader::ArchiveReaderBalancer;
use sui_storage::verify_checkpoint;

use self::{metrics::Metrics, server::CheckpointContentsDownloadLimitLayer};

/// A handle to the StateSync subsystem.
///
/// This handle can be cloned and shared. Once all copies of a StateSync system's Handle have been
/// dropped, the StateSync system will be gracefully shutdown.
#[derive(Clone, Debug)]
pub struct Handle {
    sender: mpsc::Sender<StateSyncMessage>,
    checkpoint_event_sender: broadcast::Sender<VerifiedCheckpoint>,
}

impl Handle {
    /// Send a newly minted checkpoint from Consensus to StateSync so that it can be disseminated
    /// to other nodes on the network.
    ///
    /// # Invariant
    ///
    /// Consensus must only notify StateSync of new checkpoints that have been fully committed to
    /// persistent storage. This includes CheckpointContents and all Transactions and
    /// TransactionEffects included therein.
    pub async fn send_checkpoint(&self, checkpoint: VerifiedCheckpoint) {
        self.sender
            .send(StateSyncMessage::VerifiedCheckpoint(Box::new(checkpoint)))
            .await
            .unwrap()
    }

    /// Subscribe to the stream of checkpoints that have been fully synchronized and downloaded.
    pub fn subscribe_to_synced_checkpoints(&self) -> broadcast::Receiver<VerifiedCheckpoint> {
        self.checkpoint_event_sender.subscribe()
    }
}

struct PeerHeights {
    /// Table used to track the highest checkpoint for each of our peers.
    peers: HashMap<PeerId, PeerStateSyncInfo>,
    unprocessed_checkpoints: HashMap<CheckpointDigest, Checkpoint>,
    sequence_number_to_digest: HashMap<CheckpointSequenceNumber, CheckpointDigest>,

    // The amount of time to wait before retry if there are no peers to sync content from.
    wait_interval_when_no_peer_to_sync_content: Duration,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct PeerStateSyncInfo {
    /// The digest of the Peer's genesis checkpoint.
    genesis_checkpoint_digest: CheckpointDigest,
    /// Indicates if this Peer is on the same chain as us.
    on_same_chain_as_us: bool,
    /// Highest checkpoint sequence number we know of for this Peer.
    height: CheckpointSequenceNumber,
    /// lowest available checkpoint watermark for this Peer.
    /// This defaults to 0 for now.
    lowest: CheckpointSequenceNumber,
}

impl PeerHeights {
    pub fn highest_known_checkpoint(&self) -> Option<&Checkpoint> {
        self.highest_known_checkpoint_sequence_number()
            .and_then(|s| self.sequence_number_to_digest.get(&s))
            .and_then(|digest| self.unprocessed_checkpoints.get(digest))
    }

    pub fn highest_known_checkpoint_sequence_number(&self) -> Option<CheckpointSequenceNumber> {
        self.peers
            .values()
            .filter_map(|info| info.on_same_chain_as_us.then_some(info.height))
            .max()
    }

    pub fn peers_on_same_chain(&self) -> impl Iterator<Item = (&PeerId, &PeerStateSyncInfo)> {
        self.peers
            .iter()
            .filter(|(_peer_id, info)| info.on_same_chain_as_us)
    }

    // Returns a bool that indicates if the update was done successfully.
    //
    // This will return false if the given peer doesn't have an entry or is not on the same chain
    // as us
    #[instrument(level = "debug", skip_all, fields(peer_id=?peer_id, checkpoint=?checkpoint.sequence_number()))]
    pub fn update_peer_info(
        &mut self,
        peer_id: PeerId,
        checkpoint: Checkpoint,
        low_watermark: Option<CheckpointSequenceNumber>,
    ) -> bool {
        debug!("Update peer info");

        let info = match self.peers.get_mut(&peer_id) {
            Some(info) if info.on_same_chain_as_us => info,
            _ => return false,
        };

        info.height = std::cmp::max(*checkpoint.sequence_number(), info.height);
        if let Some(low_watermark) = low_watermark {
            info.lowest = low_watermark;
        }
        self.insert_checkpoint(checkpoint);

        true
    }

    #[instrument(level = "debug", skip_all, fields(peer_id=?peer_id, lowest = ?info.lowest, height = ?info.height))]
    pub fn insert_peer_info(&mut self, peer_id: PeerId, info: PeerStateSyncInfo) {
        use std::collections::hash_map::Entry;
        debug!("Insert peer info");

        match self.peers.entry(peer_id) {
            Entry::Occupied(mut entry) => {
                // If there's already an entry and the genesis checkpoint digests match then update
                // the maximum height. Otherwise we'll use the more recent one
                let entry = entry.get_mut();
                if entry.genesis_checkpoint_digest == info.genesis_checkpoint_digest {
                    entry.height = std::cmp::max(entry.height, info.height);
                } else {
                    *entry = info;
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(info);
            }
        }
    }

    pub fn mark_peer_as_not_on_same_chain(&mut self, peer_id: PeerId) {
        if let Some(info) = self.peers.get_mut(&peer_id) {
            info.on_same_chain_as_us = false;
        }
    }

    pub fn cleanup_old_checkpoints(&mut self, sequence_number: CheckpointSequenceNumber) {
        self.unprocessed_checkpoints
            .retain(|_digest, checkpoint| *checkpoint.sequence_number() > sequence_number);
        self.sequence_number_to_digest
            .retain(|&s, _digest| s > sequence_number);
    }

    // TODO: also record who gives this checkpoint info for peer quality measurement?
    pub fn insert_checkpoint(&mut self, checkpoint: Checkpoint) {
        let digest = *checkpoint.digest();
        let sequence_number = *checkpoint.sequence_number();
        self.unprocessed_checkpoints.insert(digest, checkpoint);
        self.sequence_number_to_digest
            .insert(sequence_number, digest);
    }

    pub fn remove_checkpoint(&mut self, digest: &CheckpointDigest) {
        if let Some(checkpoint) = self.unprocessed_checkpoints.remove(digest) {
            self.sequence_number_to_digest
                .remove(checkpoint.sequence_number());
        }
    }

    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<&Checkpoint> {
        self.sequence_number_to_digest
            .get(&sequence_number)
            .and_then(|digest| self.get_checkpoint_by_digest(digest))
    }

    pub fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<&Checkpoint> {
        self.unprocessed_checkpoints.get(digest)
    }

    #[cfg(test)]
    pub fn set_wait_interval_when_no_peer_to_sync_content(&mut self, duration: Duration) {
        self.wait_interval_when_no_peer_to_sync_content = duration;
    }

    pub fn wait_interval_when_no_peer_to_sync_content(&self) -> Duration {
        self.wait_interval_when_no_peer_to_sync_content
    }
}

// PeerBalancer is an Iterator that selects peers based on RTT with some added randomness.
#[derive(Clone)]
struct PeerBalancer {
    peers: VecDeque<(anemo::Peer, PeerStateSyncInfo)>,
    requested_checkpoint: Option<CheckpointSequenceNumber>,
    request_type: PeerCheckpointRequestType,
}

#[derive(Clone)]
enum PeerCheckpointRequestType {
    Summary,
    Content,
}

impl PeerBalancer {
    pub fn new(
        network: &anemo::Network,
        peer_heights: Arc<RwLock<PeerHeights>>,
        request_type: PeerCheckpointRequestType,
    ) -> Self {
        let mut peers: Vec<_> = peer_heights
            .read()
            .unwrap()
            .peers_on_same_chain()
            // Filter out any peers who we aren't connected with.
            .filter_map(|(peer_id, info)| {
                network
                    .peer(*peer_id)
                    .map(|peer| (peer.connection_rtt(), peer, *info))
            })
            .collect();
        peers.sort_by(|(rtt_a, _, _), (rtt_b, _, _)| rtt_a.cmp(rtt_b));
        Self {
            peers: peers
                .into_iter()
                .map(|(_, peer, info)| (peer, info))
                .collect(),
            requested_checkpoint: None,
            request_type,
        }
    }

    pub fn with_checkpoint(mut self, checkpoint: CheckpointSequenceNumber) -> Self {
        self.requested_checkpoint = Some(checkpoint);
        self
    }
}

impl Iterator for PeerBalancer {
    type Item = StateSyncClient<anemo::Peer>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.peers.is_empty() {
            const SELECTION_WINDOW: usize = 2;
            let idx =
                rand::thread_rng().gen_range(0..std::cmp::min(SELECTION_WINDOW, self.peers.len()));
            let (peer, info) = self.peers.remove(idx).unwrap();
            let requested_checkpoint = self.requested_checkpoint.unwrap_or(0);
            match &self.request_type {
                // Summary will never be pruned
                PeerCheckpointRequestType::Summary if info.height >= requested_checkpoint => {
                    return Some(StateSyncClient::new(peer));
                }
                PeerCheckpointRequestType::Content
                    if info.height >= requested_checkpoint
                        && info.lowest <= requested_checkpoint =>
                {
                    return Some(StateSyncClient::new(peer));
                }
                _ => {}
            }
        }
        None
    }
}

#[derive(Clone, Debug)]
enum StateSyncMessage {
    StartSyncJob,
    // Validators will send this to the StateSyncEventLoop in order to kick off notifying our peers
    // of the new checkpoint.
    VerifiedCheckpoint(Box<VerifiedCheckpoint>),
    // Notification that the checkpoint content sync task will send to the event loop in the event
    // it was able to successfully sync a checkpoint's contents. If multiple checkpoints were
    // synced at the same time, only the highest checkpoint is sent.
    SyncedCheckpoint(Box<VerifiedCheckpoint>),
}

struct StateSyncEventLoop<S> {
    config: StateSyncConfig,

    mailbox: mpsc::Receiver<StateSyncMessage>,
    /// Weak reference to our own mailbox
    weak_sender: mpsc::WeakSender<StateSyncMessage>,

    tasks: JoinSet<()>,
    sync_checkpoint_summaries_task: Option<AbortHandle>,
    sync_checkpoint_contents_task: Option<AbortHandle>,
    download_limit_layer: Option<CheckpointContentsDownloadLimitLayer>,

    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    checkpoint_event_sender: broadcast::Sender<VerifiedCheckpoint>,
    network: anemo::Network,
    metrics: Metrics,

    archive_readers: ArchiveReaderBalancer,
    sync_checkpoint_from_archive_task: Option<AbortHandle>,
}

impl<S> StateSyncEventLoop<S>
where
    S: WriteStore + Clone + Send + Sync + 'static,
{
    // Note: A great deal of care is taken to ensure that all event handlers are non-asynchronous
    // and that the only "await" points are from the select macro picking which event to handle.
    // This ensures that the event loop is able to process events at a high speed and reduce the
    // chance for building up a backlog of events to process.
    pub async fn start(mut self) {
        info!("State-Synchronizer started");

        self.config.pinned_checkpoints.sort();

        let mut interval = tokio::time::interval(self.config.interval_period());
        let mut peer_events = {
            let (subscriber, peers) = self.network.subscribe().unwrap();
            for peer_id in peers {
                self.spawn_get_latest_from_peer(peer_id);
            }
            subscriber
        };
        let (
            target_checkpoint_contents_sequence_sender,
            target_checkpoint_contents_sequence_receiver,
        ) = watch::channel(0);

        // Spawn tokio task to update metrics periodically in the background
        let (_sender, receiver) = oneshot::channel();
        tokio::spawn(update_checkpoint_watermark_metrics(
            receiver,
            self.store.clone(),
            self.metrics.clone(),
        ));

        // Start checkpoint contents sync loop.
        let task = sync_checkpoint_contents(
            self.network.clone(),
            self.store.clone(),
            self.peer_heights.clone(),
            self.weak_sender.clone(),
            self.checkpoint_event_sender.clone(),
            self.config.checkpoint_content_download_concurrency(),
            self.config.checkpoint_content_download_tx_concurrency(),
            self.config.checkpoint_content_timeout(),
            target_checkpoint_contents_sequence_receiver,
        );
        let task_handle = self.tasks.spawn(task);
        self.sync_checkpoint_contents_task = Some(task_handle);

        // Start archive based checkpoint content sync loop.
        // TODO: Consider switching to sync from archive only on startup.
        // Right now because the peer set is fixed at startup, a node may eventually
        // end up with peers who have all purged their local state. In such a scenario it will be
        // stuck until restart when it ends up with a different set of peers. Once the discovery
        // mechanism can dynamically identify and connect to other peers on the network, we will rely
        // on sync from archive as a fall back.
        let task = sync_checkpoint_contents_from_archive(
            self.network.clone(),
            self.archive_readers.clone(),
            self.store.clone(),
            self.peer_heights.clone(),
        );
        let task_handle = self.tasks.spawn(task);
        self.sync_checkpoint_from_archive_task = Some(task_handle);

        // Start main loop.
        loop {
            tokio::select! {
                now = interval.tick() => {
                    self.handle_tick(now.into_std());
                },
                maybe_message = self.mailbox.recv() => {
                    // Once all handles to our mailbox have been dropped this
                    // will yield `None` and we can terminate the event loop
                    if let Some(message) = maybe_message {
                        self.handle_message(message);
                    } else {
                        break;
                    }
                },
                peer_event = peer_events.recv() => {
                    self.handle_peer_event(peer_event);
                },
                Some(task_result) = self.tasks.join_next() => {
                    match task_result {
                        Ok(()) => {},
                        Err(e) => {
                            if e.is_cancelled() {
                                // avoid crashing on ungraceful shutdown
                            } else if e.is_panic() {
                                // propagate panics.
                                std::panic::resume_unwind(e.into_panic());
                            } else {
                                panic!("task failed: {e}");
                            }
                        },
                    };

                    if matches!(&self.sync_checkpoint_contents_task, Some(t) if t.is_finished()) {
                        panic!("sync_checkpoint_contents task unexpectedly terminated")
                    }

                    if matches!(&self.sync_checkpoint_summaries_task, Some(t) if t.is_finished()) {
                        self.sync_checkpoint_summaries_task = None;
                    }

                    if matches!(&self.sync_checkpoint_from_archive_task, Some(t) if t.is_finished()) {
                        panic!("sync_checkpoint_from_archive task unexpectedly terminated")
                    }
                },
            }

            self.maybe_start_checkpoint_summary_sync_task();
            self.maybe_trigger_checkpoint_contents_sync_task(
                &target_checkpoint_contents_sequence_sender,
            );
        }

        info!("State-Synchronizer ended");
    }

    fn handle_message(&mut self, message: StateSyncMessage) {
        debug!("Received message: {:?}", message);
        match message {
            StateSyncMessage::StartSyncJob => self.maybe_start_checkpoint_summary_sync_task(),
            StateSyncMessage::VerifiedCheckpoint(checkpoint) => {
                self.handle_checkpoint_from_consensus(checkpoint)
            }
            // After we've successfully synced a checkpoint we can notify our peers
            StateSyncMessage::SyncedCheckpoint(checkpoint) => {
                self.spawn_notify_peers_of_checkpoint(*checkpoint)
            }
        }
    }

    // Handle a checkpoint that we received from consensus
    #[instrument(level = "debug", skip_all)]
    fn handle_checkpoint_from_consensus(&mut self, checkpoint: Box<VerifiedCheckpoint>) {
        // Always check previous_digest matches in case there is a gap between
        // state sync and consensus.
        let prev_digest = *self.store.get_checkpoint_by_sequence_number(checkpoint.sequence_number() - 1)
            .unwrap_or_else(|| panic!("Got checkpoint {} from consensus but cannot find checkpoint {} in certified_checkpoints", checkpoint.sequence_number(), checkpoint.sequence_number() - 1))
            .digest();
        if checkpoint.previous_digest != Some(prev_digest) {
            panic!("Checkpoint {} from consensus has mismatched previous_digest, expected: {:?}, actual: {:?}", checkpoint.sequence_number(), Some(prev_digest), checkpoint.previous_digest);
        }

        let latest_checkpoint = self
            .store
            .get_highest_verified_checkpoint()
            .expect("store operation should not fail");

        // If this is an older checkpoint, just ignore it
        if latest_checkpoint.sequence_number() >= checkpoint.sequence_number() {
            return;
        }

        let checkpoint = *checkpoint;
        let next_sequence_number = latest_checkpoint.sequence_number().checked_add(1).unwrap();
        if *checkpoint.sequence_number() > next_sequence_number {
            debug!(
                "consensus sent too new of a checkpoint, expecting: {}, got: {}",
                next_sequence_number,
                checkpoint.sequence_number()
            );
        }

        // Because checkpoint from consensus sends in order, when we have checkpoint n,
        // we must have all of the checkpoints before n from either state sync or consensus.
        #[cfg(debug_assertions)]
        {
            let _ = (next_sequence_number..=*checkpoint.sequence_number())
                .map(|n| {
                    let checkpoint = self
                        .store
                        .get_checkpoint_by_sequence_number(n)
                        .unwrap_or_else(|| panic!("store should contain checkpoint {n}"));
                    self.store
                        .get_full_checkpoint_contents(&checkpoint.content_digest)
                        .unwrap_or_else(|| {
                            panic!(
                                "store should contain checkpoint contents for {:?}",
                                checkpoint.content_digest
                            )
                        });
                })
                .collect::<Vec<_>>();
        }

        // If this is the last checkpoint of a epoch, we need to make sure
        // new committee is in store before we verify newer checkpoints in next epoch.
        // This could happen before this validator's reconfiguration finishes, because
        // state sync does not reconfig.
        // TODO: make CheckpointAggregator use WriteStore so we don't need to do this
        // committee insertion in two places (only in `insert_checkpoint`).
        if let Some(EndOfEpochData {
            next_epoch_committee,
            ..
        }) = checkpoint.end_of_epoch_data.as_ref()
        {
            let next_committee = next_epoch_committee.iter().cloned().collect();
            let committee =
                Committee::new(checkpoint.epoch().checked_add(1).unwrap(), next_committee);
            self.store
                .insert_committee(committee)
                .expect("store operation should not fail");
        }

        self.store
            .update_highest_verified_checkpoint(&checkpoint)
            .expect("store operation should not fail");
        self.store
            .update_highest_synced_checkpoint(&checkpoint)
            .expect("store operation should not fail");

        // We don't care if no one is listening as this is a broadcast channel
        let _ = self.checkpoint_event_sender.send(checkpoint.clone());

        self.spawn_notify_peers_of_checkpoint(checkpoint);
    }

    fn handle_peer_event(
        &mut self,
        peer_event: Result<PeerEvent, tokio::sync::broadcast::error::RecvError>,
    ) {
        use tokio::sync::broadcast::error::RecvError;

        match peer_event {
            Ok(PeerEvent::NewPeer(peer_id)) => {
                self.spawn_get_latest_from_peer(peer_id);
            }
            Ok(PeerEvent::LostPeer(peer_id, _)) => {
                self.peer_heights.write().unwrap().peers.remove(&peer_id);
            }

            Err(RecvError::Closed) => {
                panic!("PeerEvent channel shouldn't be able to be closed");
            }

            Err(RecvError::Lagged(_)) => {
                trace!("State-Sync fell behind processing PeerEvents");
            }
        }
    }

    fn spawn_get_latest_from_peer(&mut self, peer_id: PeerId) {
        if let Some(peer) = self.network.peer(peer_id) {
            let genesis_checkpoint_digest = *self
                .store
                .get_checkpoint_by_sequence_number(0)
                .expect("store should contain genesis checkpoint")
                .digest();
            let task = get_latest_from_peer(
                genesis_checkpoint_digest,
                peer,
                self.peer_heights.clone(),
                self.config.timeout(),
            );
            self.tasks.spawn(task);
        }
    }

    fn handle_tick(&mut self, _now: std::time::Instant) {
        let task = query_peers_for_their_latest_checkpoint(
            self.network.clone(),
            self.peer_heights.clone(),
            self.weak_sender.clone(),
            self.config.timeout(),
        );
        self.tasks.spawn(task);

        if let Some(layer) = self.download_limit_layer.as_ref() {
            layer.maybe_prune_map();
        }
    }

    fn maybe_start_checkpoint_summary_sync_task(&mut self) {
        // Only run one sync task at a time
        if self.sync_checkpoint_summaries_task.is_some() {
            return;
        }

        let highest_processed_checkpoint = self
            .store
            .get_highest_verified_checkpoint()
            .expect("store operation should not fail");

        let highest_known_checkpoint = self
            .peer_heights
            .read()
            .unwrap()
            .highest_known_checkpoint()
            .cloned();

        if Some(highest_processed_checkpoint.sequence_number())
            < highest_known_checkpoint
                .as_ref()
                .map(|x| x.sequence_number())
        {
            // start sync job
            let task = sync_to_checkpoint(
                self.network.clone(),
                self.store.clone(),
                self.peer_heights.clone(),
                self.metrics.clone(),
                self.config.pinned_checkpoints.clone(),
                self.config.checkpoint_header_download_concurrency(),
                self.config.timeout(),
                // The if condition should ensure that this is Some
                highest_known_checkpoint.unwrap(),
            )
            .map(|result| match result {
                Ok(()) => {}
                Err(e) => {
                    debug!("error syncing checkpoint {e}");
                }
            });
            let task_handle = self.tasks.spawn(task);
            self.sync_checkpoint_summaries_task = Some(task_handle);
        }
    }

    fn maybe_trigger_checkpoint_contents_sync_task(
        &mut self,
        target_sequence_channel: &watch::Sender<CheckpointSequenceNumber>,
    ) {
        let highest_verified_checkpoint = self
            .store
            .get_highest_verified_checkpoint()
            .expect("store operation should not fail");
        let highest_synced_checkpoint = self
            .store
            .get_highest_synced_checkpoint()
            .expect("store operation should not fail");

        if highest_verified_checkpoint.sequence_number()
            > highest_synced_checkpoint.sequence_number()
            // skip if we aren't connected to any peers that can help
            && self
                .peer_heights
                .read()
                .unwrap()
                .highest_known_checkpoint_sequence_number()
                > Some(*highest_synced_checkpoint.sequence_number())
        {
            let _ = target_sequence_channel.send_if_modified(|num| {
                let new_num = *highest_verified_checkpoint.sequence_number();
                if *num == new_num {
                    return false;
                }
                *num = new_num;
                true
            });
        }
    }

    fn spawn_notify_peers_of_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        let task = notify_peers_of_checkpoint(
            self.network.clone(),
            self.peer_heights.clone(),
            checkpoint,
            self.config.timeout(),
        );
        self.tasks.spawn(task);
    }
}

async fn notify_peers_of_checkpoint(
    network: anemo::Network,
    peer_heights: Arc<RwLock<PeerHeights>>,
    checkpoint: VerifiedCheckpoint,
    timeout: Duration,
) {
    let futs = peer_heights
        .read()
        .unwrap()
        .peers_on_same_chain()
        // Filter out any peers who we know already have a checkpoint higher than this one
        .filter_map(|(peer_id, info)| {
            (*checkpoint.sequence_number() > info.height).then_some(peer_id)
        })
        // Filter out any peers who we aren't connected with
        .flat_map(|peer_id| network.peer(*peer_id))
        .map(StateSyncClient::new)
        .map(|mut client| {
            let request = Request::new(checkpoint.inner().clone()).with_timeout(timeout);
            async move { client.push_checkpoint_summary(request).await }
        })
        .collect::<Vec<_>>();
    futures::future::join_all(futs).await;
}

async fn get_latest_from_peer(
    our_genesis_checkpoint_digest: CheckpointDigest,
    peer: anemo::Peer,
    peer_heights: Arc<RwLock<PeerHeights>>,
    timeout: Duration,
) {
    let peer_id = peer.peer_id();
    let mut client = StateSyncClient::new(peer);

    let info = {
        let maybe_info = peer_heights.read().unwrap().peers.get(&peer_id).copied();

        if let Some(info) = maybe_info {
            info
        } else {
            // TODO do we want to create a new API just for querying a node's chainid?
            //
            // We need to query this node's genesis checkpoint to see if they're on the same chain
            // as us
            let request = Request::new(GetCheckpointSummaryRequest::BySequenceNumber(0))
                .with_timeout(timeout);
            let response = client
                .get_checkpoint_summary(request)
                .await
                .map(Response::into_inner);

            let info = match response {
                Ok(Some(checkpoint)) => {
                    let digest = *checkpoint.digest();
                    PeerStateSyncInfo {
                        genesis_checkpoint_digest: digest,
                        on_same_chain_as_us: our_genesis_checkpoint_digest == digest,
                        height: *checkpoint.sequence_number(),
                        lowest: CheckpointSequenceNumber::default(),
                    }
                }
                Ok(None) => PeerStateSyncInfo {
                    genesis_checkpoint_digest: CheckpointDigest::default(),
                    on_same_chain_as_us: false,
                    height: CheckpointSequenceNumber::default(),
                    lowest: CheckpointSequenceNumber::default(),
                },
                Err(status) => {
                    trace!("get_latest_checkpoint_summary request failed: {status:?}");
                    return;
                }
            };
            peer_heights
                .write()
                .unwrap()
                .insert_peer_info(peer_id, info);
            info
        }
    };

    // Bail early if this node isn't on the same chain as us
    if !info.on_same_chain_as_us {
        trace!(?info, "Peer {peer_id} not on same chain as us");
        return;
    }
    let Some((highest_checkpoint, low_watermark)) =
        query_peer_for_latest_info(&mut client, timeout).await
    else {
        return;
    };
    peer_heights
        .write()
        .unwrap()
        .update_peer_info(peer_id, highest_checkpoint, low_watermark);
}

/// Queries a peer for their highest_synced_checkpoint and low checkpoint watermark
async fn query_peer_for_latest_info(
    client: &mut StateSyncClient<anemo::Peer>,
    timeout: Duration,
) -> Option<(Checkpoint, Option<CheckpointSequenceNumber>)> {
    let request = Request::new(()).with_timeout(timeout);
    let response = client
        .get_checkpoint_availability(request)
        .await
        .map(Response::into_inner);
    match response {
        Ok(GetCheckpointAvailabilityResponse {
            highest_synced_checkpoint,
            lowest_available_checkpoint,
        }) => {
            return Some((highest_synced_checkpoint, Some(lowest_available_checkpoint)));
        }
        Err(status) => {
            // If peer hasn't upgraded they would return 404 NotFound error
            if status.status() != anemo::types::response::StatusCode::NotFound {
                trace!("get_checkpoint_availability request failed: {status:?}");
                return None;
            }
        }
    };

    // Then we try the old query
    // TODO: remove this once the new feature stabilizes
    let request = Request::new(GetCheckpointSummaryRequest::Latest).with_timeout(timeout);
    let response = client
        .get_checkpoint_summary(request)
        .await
        .map(Response::into_inner);
    match response {
        Ok(Some(checkpoint)) => Some((checkpoint, None)),
        Ok(None) => None,
        Err(status) => {
            trace!("get_checkpoint_summary (latest) request failed: {status:?}");
            None
        }
    }
}

#[instrument(level = "debug", skip_all)]
async fn query_peers_for_their_latest_checkpoint(
    network: anemo::Network,
    peer_heights: Arc<RwLock<PeerHeights>>,
    sender: mpsc::WeakSender<StateSyncMessage>,
    timeout: Duration,
) {
    let peer_heights = &peer_heights;
    let futs = peer_heights
        .read()
        .unwrap()
        .peers_on_same_chain()
        // Filter out any peers who we aren't connected with
        .flat_map(|(peer_id, _info)| network.peer(*peer_id))
        .map(|peer| {
            let peer_id = peer.peer_id();
            let mut client = StateSyncClient::new(peer);

            async move {
                let response = query_peer_for_latest_info(&mut client, timeout).await;
                match response {
                    Some((highest_checkpoint, low_watermark)) => peer_heights
                        .write()
                        .unwrap()
                        .update_peer_info(peer_id, highest_checkpoint.clone(), low_watermark)
                        .then_some(highest_checkpoint),
                    None => None,
                }
            }
        })
        .collect::<Vec<_>>();

    debug!("Query {} peers for latest checkpoint", futs.len());

    let checkpoints = futures::future::join_all(futs).await.into_iter().flatten();

    let highest_checkpoint = checkpoints.max_by_key(|checkpoint| *checkpoint.sequence_number());

    let our_highest_checkpoint = peer_heights
        .read()
        .unwrap()
        .highest_known_checkpoint()
        .cloned();

    debug!(
        "Our highest checkpoint {:?}, peers highest checkpoint {:?}",
        our_highest_checkpoint.as_ref().map(|c| c.sequence_number()),
        highest_checkpoint.as_ref().map(|c| c.sequence_number())
    );

    let _new_checkpoint = match (highest_checkpoint, our_highest_checkpoint) {
        (Some(theirs), None) => theirs,
        (Some(theirs), Some(ours)) if theirs.sequence_number() > ours.sequence_number() => theirs,
        _ => return,
    };

    if let Some(sender) = sender.upgrade() {
        let _ = sender.send(StateSyncMessage::StartSyncJob).await;
    }
}

async fn sync_to_checkpoint<S>(
    network: anemo::Network,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    metrics: Metrics,
    pinned_checkpoints: Vec<(CheckpointSequenceNumber, CheckpointDigest)>,
    checkpoint_header_download_concurrency: usize,
    timeout: Duration,
    checkpoint: Checkpoint,
) -> Result<()>
where
    S: WriteStore,
{
    metrics.set_highest_known_checkpoint(*checkpoint.sequence_number());

    let mut current = store
        .get_highest_verified_checkpoint()
        .expect("store operation should not fail");
    if current.sequence_number() >= checkpoint.sequence_number() {
        return Err(anyhow::anyhow!(
            "target checkpoint {} is older than highest verified checkpoint {}",
            checkpoint.sequence_number(),
            current.sequence_number(),
        ));
    }

    let peer_balancer = PeerBalancer::new(
        &network,
        peer_heights.clone(),
        PeerCheckpointRequestType::Summary,
    );
    // range of the next sequence_numbers to fetch
    let mut request_stream = (current.sequence_number().checked_add(1).unwrap()
        ..=*checkpoint.sequence_number())
        .map(|next| {
            let peers = peer_balancer.clone().with_checkpoint(next);
            let peer_heights = peer_heights.clone();
            let pinned_checkpoints = &pinned_checkpoints;
            async move {
                if let Some(checkpoint) = peer_heights
                    .read()
                    .unwrap()
                    .get_checkpoint_by_sequence_number(next)
                {
                    return (Some(checkpoint.to_owned()), next, None);
                }

                // Iterate through peers trying each one in turn until we're able to
                // successfully get the target checkpoint
                for mut peer in peers {
                    let request = Request::new(GetCheckpointSummaryRequest::BySequenceNumber(next))
                        .with_timeout(timeout);
                    if let Some(checkpoint) = peer
                        .get_checkpoint_summary(request)
                        .await
                        .tap_err(|e| trace!("{e:?}"))
                        .ok()
                        .and_then(Response::into_inner)
                        .tap_none(|| trace!("peer unable to help sync"))
                    {
                        // peer didn't give us a checkpoint with the height that we requested
                        if *checkpoint.sequence_number() != next {
                            tracing::debug!(
                                "peer returned checkpoint with wrong sequence number: expected {next}, got {}",
                                checkpoint.sequence_number()
                            );
                            continue;
                        }

                        // peer gave us a checkpoint whose digest does not match pinned digest
                        let checkpoint_digest = checkpoint.digest();
                        if let Ok(pinned_digest_index) = pinned_checkpoints.binary_search_by_key(
                            checkpoint.sequence_number(),
                            |(seq_num, _digest)| *seq_num
                        ) {
                            if pinned_checkpoints[pinned_digest_index].1 != *checkpoint_digest {
                                tracing::debug!(
                                    "peer returned checkpoint with digest that does not match pinned digest: expected {:?}, got {:?}",
                                    pinned_checkpoints[pinned_digest_index].1,
                                    checkpoint_digest
                                );
                                continue;
                            }
                        }

                        // Insert in our store in the event that things fail and we need to retry
                        peer_heights
                            .write()
                            .unwrap()
                            .insert_checkpoint(checkpoint.clone());
                        return (Some(checkpoint), next, Some(peer.inner().peer_id()));
                    }
                }
                (None, next, None)
            }
        })
        .pipe(futures::stream::iter)
        .buffered(checkpoint_header_download_concurrency);

    while let Some((maybe_checkpoint, next, maybe_peer_id)) = request_stream.next().await {
        assert_eq!(
            current
                .sequence_number()
                .checked_add(1)
                .expect("exhausted u64"),
            next
        );

        // Verify the checkpoint
        let checkpoint = 'cp: {
            let checkpoint = maybe_checkpoint.ok_or_else(|| {
                anyhow::anyhow!("no peers were able to help sync checkpoint {next}")
            })?;
            // Skip verification for manually pinned checkpoints.
            if pinned_checkpoints
                .binary_search_by_key(checkpoint.sequence_number(), |(seq_num, _digest)| *seq_num)
                .is_ok()
            {
                break 'cp VerifiedCheckpoint::new_unchecked(checkpoint);
            }
            match verify_checkpoint(&current, &store, checkpoint) {
                Ok(verified_checkpoint) => verified_checkpoint,
                Err(checkpoint) => {
                    let mut peer_heights = peer_heights.write().unwrap();
                    // Remove the checkpoint from our temporary store so that we can try querying
                    // another peer for a different one
                    peer_heights.remove_checkpoint(checkpoint.digest());

                    // Mark peer as not on the same chain as us
                    if let Some(peer_id) = maybe_peer_id {
                        peer_heights.mark_peer_as_not_on_same_chain(peer_id);
                    }

                    return Err(anyhow::anyhow!(
                        "unable to verify checkpoint {checkpoint:?}"
                    ));
                }
            }
        };

        debug!(checkpoint_seq = ?checkpoint.sequence_number(), "verified checkpoint summary");
        if let Some((checkpoint_summary_age_metric, checkpoint_summary_age_metric_deprecated)) =
            metrics.checkpoint_summary_age_metrics()
        {
            checkpoint.report_checkpoint_age(
                checkpoint_summary_age_metric,
                checkpoint_summary_age_metric_deprecated,
            );
        }

        current = checkpoint.clone();
        // Insert the newly verified checkpoint into our store, which will bump our highest
        // verified checkpoint watermark as well.
        store
            .insert_checkpoint(&checkpoint)
            .expect("store operation should not fail");
    }

    peer_heights
        .write()
        .unwrap()
        .cleanup_old_checkpoints(*checkpoint.sequence_number());

    Ok(())
}

async fn sync_checkpoint_contents_from_archive<S>(
    network: anemo::Network,
    archive_readers: ArchiveReaderBalancer,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
) where
    S: WriteStore + Clone + Send + Sync + 'static,
{
    loop {
        let peers: Vec<_> = peer_heights
            .read()
            .unwrap()
            .peers_on_same_chain()
            // Filter out any peers who we aren't connected with.
            .filter_map(|(peer_id, info)| network.peer(*peer_id).map(|peer| (peer, *info)))
            .collect();
        let lowest_checkpoint_on_peers = peers
            .iter()
            .map(|(_p, state_sync_info)| state_sync_info.lowest)
            .min();
        let highest_synced = store
            .get_highest_synced_checkpoint()
            .expect("store operation should not fail")
            .sequence_number;
        let sync_from_archive = if let Some(lowest_checkpoint_on_peers) = lowest_checkpoint_on_peers
        {
            highest_synced < lowest_checkpoint_on_peers
        } else {
            false
        };
        debug!("Syncing checkpoint contents from archive: {sync_from_archive},  highest_synced: {highest_synced},  lowest_checkpoint_on_peers: {}", lowest_checkpoint_on_peers.map_or_else(|| "None".to_string(), |l| l.to_string()));
        if sync_from_archive {
            let start = highest_synced
                .checked_add(1)
                .expect("Checkpoint seq num overflow");
            let checkpoint_range = start..lowest_checkpoint_on_peers.unwrap();
            if let Some(archive_reader) = archive_readers
                .pick_one_random(checkpoint_range.clone())
                .await
            {
                let txn_counter = Arc::new(AtomicU64::new(0));
                let checkpoint_counter = Arc::new(AtomicU64::new(0));
                if let Err(err) = archive_reader
                    .read(
                        store.clone(),
                        checkpoint_range,
                        txn_counter.clone(),
                        checkpoint_counter.clone(),
                        true,
                    )
                    .await
                {
                    warn!("State sync from archive failed with error: {:?}", err);
                } else {
                    info!("State sync from archive is complete. Checkpoints downloaded = {:?}, Txns downloaded = {:?}", checkpoint_counter.load(Ordering::Relaxed), txn_counter.load(Ordering::Relaxed));
                }
            } else {
                warn!("Failed to find an archive reader to complete the state sync request");
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn sync_checkpoint_contents<S>(
    network: anemo::Network,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    sender: mpsc::WeakSender<StateSyncMessage>,
    checkpoint_event_sender: broadcast::Sender<VerifiedCheckpoint>,
    checkpoint_content_download_concurrency: usize,
    checkpoint_content_download_tx_concurrency: u64,
    timeout: Duration,
    mut target_sequence_channel: watch::Receiver<CheckpointSequenceNumber>,
) where
    S: WriteStore + Clone,
{
    let mut highest_synced = store
        .get_highest_synced_checkpoint()
        .expect("store operation should not fail");

    let mut current_sequence = highest_synced.sequence_number().checked_add(1).unwrap();
    let mut target_sequence_cursor = 0;
    let mut highest_started_network_total_transactions = highest_synced.network_total_transactions;
    let mut checkpoint_contents_tasks = FuturesOrdered::new();

    let mut tx_concurrency_remaining = checkpoint_content_download_tx_concurrency;

    loop {
        tokio::select! {
            result = target_sequence_channel.changed() => {
                match result {
                    Ok(()) => {
                        target_sequence_cursor = (*target_sequence_channel.borrow_and_update()).checked_add(1).unwrap();
                    }
                    Err(_) => {
                        // Watch channel is closed, exit loop.
                        return
                    }
                }
            },
            Some(maybe_checkpoint) = checkpoint_contents_tasks.next() => {
                match maybe_checkpoint {
                    Ok(checkpoint) => {
                        let _: &VerifiedCheckpoint = &checkpoint;  // type hint

                        store
                            .update_highest_synced_checkpoint(&checkpoint)
                            .expect("store operation should not fail");
                        // We don't care if no one is listening as this is a broadcast channel
                        let _ = checkpoint_event_sender.send(checkpoint.clone());
                        tx_concurrency_remaining += checkpoint.network_total_transactions - highest_synced.network_total_transactions;
                        highest_synced = checkpoint;

                    }
                    Err(checkpoint) => {
                        let _: &VerifiedCheckpoint = &checkpoint;  // type hint
                        if let Some(lowest_peer_checkpoint) =
                            peer_heights.read().ok().and_then(|x| x.peers.values().map(|state_sync_info| state_sync_info.lowest).min()) {
                            if checkpoint.sequence_number() >= &lowest_peer_checkpoint {
                                info!("unable to sync contents of checkpoint through state sync {} with lowest peer checkpoint: {}", checkpoint.sequence_number(), lowest_peer_checkpoint);
                            }
                        } else {
                            info!("unable to sync contents of checkpoint through state sync {}", checkpoint.sequence_number());

                        }
                        // Retry contents sync on failure.
                        checkpoint_contents_tasks.push_front(sync_one_checkpoint_contents(
                            network.clone(),
                            &store,
                            peer_heights.clone(),
                            timeout,
                            checkpoint,
                        ));
                    }
                }
            },
        }

        // Start new tasks up to configured concurrency limits.
        while current_sequence < target_sequence_cursor
            && checkpoint_contents_tasks.len() < checkpoint_content_download_concurrency
        {
            let next_checkpoint = store
                .get_checkpoint_by_sequence_number(current_sequence)
                .expect(
                    "BUG: store should have all checkpoints older than highest_verified_checkpoint",
                );

            // Enforce transaction count concurrency limit.
            let tx_count = next_checkpoint.network_total_transactions
                - highest_started_network_total_transactions;
            if tx_count > tx_concurrency_remaining {
                break;
            }
            tx_concurrency_remaining -= tx_count;

            highest_started_network_total_transactions = next_checkpoint.network_total_transactions;
            current_sequence += 1;
            checkpoint_contents_tasks.push_back(sync_one_checkpoint_contents(
                network.clone(),
                &store,
                peer_heights.clone(),
                timeout,
                next_checkpoint,
            ));
        }

        if highest_synced.sequence_number() % checkpoint_content_download_concurrency as u64 == 0
            || checkpoint_contents_tasks.is_empty()
        {
            // Periodically notify event loop to notify our peers that we've synced to a new checkpoint height
            if let Some(sender) = sender.upgrade() {
                let message = StateSyncMessage::SyncedCheckpoint(Box::new(highest_synced.clone()));
                let _ = sender.send(message).await;
            }
        }
    }
}

#[instrument(level = "debug", skip_all, fields(sequence_number = ?checkpoint.sequence_number()))]
async fn sync_one_checkpoint_contents<S>(
    network: anemo::Network,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    timeout: Duration,
    checkpoint: VerifiedCheckpoint,
) -> Result<VerifiedCheckpoint, VerifiedCheckpoint>
where
    S: WriteStore + Clone,
{
    debug!("syncing checkpoint contents");

    // Check if we already have produced this checkpoint locally. If so, we don't need
    // to get it from peers anymore.
    if store
        .get_highest_synced_checkpoint()
        .expect("store operation should not fail")
        .sequence_number()
        >= checkpoint.sequence_number()
    {
        debug!("checkpoint was already created via consensus output");
        return Ok(checkpoint);
    }

    // Request checkpoint contents from peers.
    let peers = PeerBalancer::new(
        &network,
        peer_heights.clone(),
        PeerCheckpointRequestType::Content,
    )
    .with_checkpoint(*checkpoint.sequence_number());
    let now = tokio::time::Instant::now();
    let Some(_contents) = get_full_checkpoint_contents(peers, &store, &checkpoint, timeout).await
    else {
        // Delay completion in case of error so we don't hammer the network with retries.
        let duration = peer_heights
            .read()
            .unwrap()
            .wait_interval_when_no_peer_to_sync_content();
        if now.elapsed() < duration {
            let duration = duration - now.elapsed();
            info!("retrying checkpoint sync after {:?}", duration);
            tokio::time::sleep(duration).await;
        }
        return Err(checkpoint);
    };
    debug!("completed checkpoint contents sync");
    Ok(checkpoint)
}

#[instrument(level = "debug", skip_all)]
async fn get_full_checkpoint_contents<S>(
    peers: PeerBalancer,
    store: S,
    checkpoint: &VerifiedCheckpoint,
    timeout: Duration,
) -> Option<FullCheckpointContents>
where
    S: WriteStore,
{
    let digest = checkpoint.content_digest;
    if let Some(contents) = store
        .get_full_checkpoint_contents_by_sequence_number(*checkpoint.sequence_number())
        .or_else(|| store.get_full_checkpoint_contents(&digest))
    {
        debug!("store already contains checkpoint contents");
        return Some(contents);
    }

    // Iterate through our selected peers trying each one in turn until we're able to
    // successfully get the target checkpoint
    for mut peer in peers {
        debug!(
            ?timeout,
            "requesting checkpoint contents from {}",
            peer.inner().peer_id(),
        );
        let request = Request::new(digest).with_timeout(timeout);
        if let Some(contents) = peer
            .get_checkpoint_contents(request)
            .await
            .tap_err(|e| trace!("{e:?}"))
            .ok()
            .and_then(Response::into_inner)
            .tap_none(|| trace!("peer unable to help sync"))
        {
            if contents.verify_digests(digest).is_ok() {
                let verified_contents = VerifiedCheckpointContents::new_unchecked(contents.clone());
                store
                    .insert_checkpoint_contents(checkpoint, verified_contents)
                    .expect("store operation should not fail");
                return Some(contents);
            }
        }
    }
    debug!("no peers had checkpoint contents");
    None
}

async fn update_checkpoint_watermark_metrics<S>(
    mut recv: oneshot::Receiver<()>,
    store: S,
    metrics: Metrics,
) -> Result<()>
where
    S: WriteStore + Clone + Send + Sync,
{
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
             _now = interval.tick() => {
                let highest_verified_checkpoint = store.get_highest_verified_checkpoint()
                    .expect("store operation should not fail");
                metrics.set_highest_verified_checkpoint(highest_verified_checkpoint.sequence_number);
                let highest_synced_checkpoint = store.get_highest_synced_checkpoint()
                    .expect("store operation should not fail");
                metrics.set_highest_synced_checkpoint(highest_synced_checkpoint.sequence_number);
             },
            _ = &mut recv => break,
        }
    }
    Ok(())
}
