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
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime},
};
use sui_config::p2p::StateSyncConfig;
use sui_types::{
    digests::CheckpointDigest,
    messages_checkpoint::{
        CertifiedCheckpointSummary as Checkpoint, CheckpointSequenceNumber, FullCheckpointContents,
        VerifiedCheckpoint, VerifiedCheckpointContents,
    },
    storage::ReadStore,
    storage::WriteStore,
};
use tap::{Pipe, TapFallible, TapOptional};
use tokio::{
    sync::{broadcast, mpsc, watch},
    task::{AbortHandle, JoinSet},
};
use tracing::{debug, info, trace, warn};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/sui.StateSync.rs"));
}
mod builder;
mod metrics;
mod server;
pub mod test_utils;
#[cfg(test)]
mod tests;

pub use builder::{Builder, UnstartedStateSync};
pub use generated::{
    state_sync_client::StateSyncClient,
    state_sync_server::{StateSync, StateSyncServer},
};
pub use server::GetCheckpointSummaryRequest;

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
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct PeerStateSyncInfo {
    /// The digest of the Peer's genesis checkpoint.
    genesis_checkpoint_digest: CheckpointDigest,
    /// Indicates if this Peer is on the same chain as us.
    on_same_chain_as_us: bool,
    /// Highest checkpoint sequence number we know of for this Peer.
    height: CheckpointSequenceNumber,
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
    pub fn update_peer_info(&mut self, peer_id: PeerId, checkpoint: Checkpoint) -> bool {
        let info = match self.peers.get_mut(&peer_id) {
            Some(info) if info.on_same_chain_as_us => info,
            _ => return false,
        };

        info.height = std::cmp::max(*checkpoint.sequence_number(), info.height);
        self.insert_checkpoint(checkpoint);

        true
    }

    pub fn insert_peer_info(&mut self, peer_id: PeerId, info: PeerStateSyncInfo) {
        use std::collections::hash_map::Entry;

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
}

impl<S> StateSyncEventLoop<S>
where
    S: WriteStore + Clone + Send + Sync + 'static,
    <S as ReadStore>::Error: std::error::Error,
{
    // Note: A great deal of care is taken to ensure that all event handlers are non-asynchronous
    // and that the only "await" points are from the select macro picking which event to handle.
    // This ensures that the event loop is able to process events at a high speed and reduce the
    // chance for building up a backlog of events to process.
    pub async fn start(mut self) {
        info!("State-Synchronizer started");

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

        // Initialize checkpoint watermark metrics
        self.metrics.set_highest_verified_checkpoint(
            *self
                .store
                .get_highest_verified_checkpoint()
                .expect("store operation should not fail")
                .sequence_number(),
        );
        self.metrics.set_highest_synced_checkpoint(
            *self
                .store
                .get_highest_synced_checkpoint()
                .expect("store operation should not fail")
                .sequence_number(),
        );

        // Start checkpoint contents sync loop.
        let task = sync_checkpoint_contents(
            self.network.clone(),
            self.store.clone(),
            self.peer_heights.clone(),
            self.weak_sender.clone(),
            self.checkpoint_event_sender.clone(),
            self.metrics.clone(),
            self.config.checkpoint_content_download_concurrency(),
            self.config.checkpoint_content_download_tx_concurrency(),
            self.config.checkpoint_content_timeout(),
            target_checkpoint_contents_sequence_receiver,
        );
        let task_handle = self.tasks.spawn(task);
        self.sync_checkpoint_contents_task = Some(task_handle);

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
    fn handle_checkpoint_from_consensus(&mut self, checkpoint: Box<VerifiedCheckpoint>) {
        let (next_sequence_number, previous_digest) = {
            let latest_checkpoint = self
                .store
                .get_highest_verified_checkpoint()
                .expect("store operation should not fail");

            // If this is an older checkpoint, just ignore it
            if latest_checkpoint.sequence_number() >= checkpoint.sequence_number() {
                return;
            }

            let next_sequence_number = latest_checkpoint.sequence_number().saturating_add(1);
            let previous_digest = *latest_checkpoint.digest();
            (next_sequence_number, previous_digest)
        };

        // If this is exactly the next checkpoint then insert it and then notify our peers
        if *checkpoint.sequence_number() == next_sequence_number
            && checkpoint.previous_digest == Some(previous_digest)
        {
            let checkpoint = *checkpoint;

            // Check invariant that consensus must only send state-sync fully synced checkpoints
            #[cfg(debug_assertions)]
            {
                self.store
                    .get_full_checkpoint_contents(&checkpoint.content_digest)
                    .expect("store operation should not fail")
                    .unwrap();
            }

            self.store
                .insert_checkpoint(checkpoint.clone())
                .expect("store operation should not fail");
            self.store
                .update_highest_synced_checkpoint(&checkpoint)
                .expect("store operation should not fail");
            self.metrics
                .set_highest_verified_checkpoint(*checkpoint.sequence_number());
            self.metrics
                .set_highest_synced_checkpoint(*checkpoint.sequence_number());

            // We don't care if no one is listening as this is a broadcast channel
            let _ = self.checkpoint_event_sender.send(checkpoint.clone());

            self.spawn_notify_peers_of_checkpoint(checkpoint);
        } else {
            // Ensure that if consensus sends us a checkpoint that we expect to be the next one,
            // that it isn't on a fork
            if *checkpoint.sequence_number() == next_sequence_number {
                assert_eq!(checkpoint.previous_digest, Some(previous_digest));
            }

            debug!("consensus sent too new of a checkpoint");

            // See if the missing checkpoints are already in our store and quickly update our
            // watermarks
            let mut checkpoints_from_storage =
                (next_sequence_number..=*checkpoint.sequence_number()).map(|n| {
                    self.store
                        .get_checkpoint_by_sequence_number(n)
                        .expect("store operation should not fail")
                });
            while let Some(Some(checkpoint)) = checkpoints_from_storage.next() {
                self.store
                    .insert_checkpoint(checkpoint.clone())
                    .expect("store operation should not fail");
                self.store
                    .update_highest_synced_checkpoint(&checkpoint)
                    .expect("store operation should not fail");
                self.metrics
                    .set_highest_verified_checkpoint(*checkpoint.sequence_number());
                self.metrics
                    .set_highest_synced_checkpoint(*checkpoint.sequence_number());

                // We don't care if no one is listening as this is a broadcast channel
                let _ = self.checkpoint_event_sender.send(checkpoint.clone());
            }
        }
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
                .expect("store operation should not fail")
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
                    }
                }
                Ok(None) => PeerStateSyncInfo {
                    genesis_checkpoint_digest: CheckpointDigest::default(),
                    on_same_chain_as_us: false,
                    height: CheckpointSequenceNumber::default(),
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
        return;
    }

    let checkpoint = {
        let request = Request::new(GetCheckpointSummaryRequest::Latest).with_timeout(timeout);
        let response = client
            .get_checkpoint_summary(request)
            .await
            .map(Response::into_inner);
        match response {
            Ok(Some(checkpoint)) => checkpoint,
            Ok(None) => return,
            Err(status) => {
                trace!("get_latest_checkpoint_summary request failed: {status:?}");
                return;
            }
        }
    };

    peer_heights
        .write()
        .unwrap()
        .update_peer_info(peer_id, checkpoint);
}

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
                let request =
                    Request::new(GetCheckpointSummaryRequest::Latest).with_timeout(timeout);
                let response = client
                    .get_checkpoint_summary(request)
                    .await
                    .map(Response::into_inner);
                match response {
                    Ok(Some(checkpoint)) => peer_heights
                        .write()
                        .unwrap()
                        .update_peer_info(peer_id, checkpoint.clone())
                        .then_some(checkpoint),
                    Ok(None) => None,
                    Err(status) => {
                        trace!("get_latest_checkpoint_summary request failed: {status:?}");
                        None
                    }
                }
            }
        })
        .collect::<Vec<_>>();

    let checkpoints = futures::future::join_all(futs).await.into_iter().flatten();

    let highest_checkpoint = checkpoints.max_by_key(|checkpoint| *checkpoint.sequence_number());

    let our_highest_checkpoint = peer_heights
        .read()
        .unwrap()
        .highest_known_checkpoint()
        .cloned();

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
    checkpoint_header_download_concurrency: usize,
    timeout: Duration,
    checkpoint: Checkpoint,
) -> Result<()>
where
    S: WriteStore,
    <S as ReadStore>::Error: std::error::Error,
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

    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_entropy();
    // get a list of peers that can help
    let peers = peer_heights
        .read()
        .unwrap()
        .peers_on_same_chain()
        // Filter out any peers who can't help
        .filter(|(_peer_id, info)| info.height > *current.sequence_number())
        .map(|(&peer_id, &info)| (peer_id, info))
        .collect::<Vec<_>>();

    // range of the next sequence_numbers to fetch
    let mut request_stream = (current.sequence_number().saturating_add(1)
        ..=*checkpoint.sequence_number())
        .map(|next| {
            let mut peers = peers
                .iter()
                // Filter out any peers who can't help with this particular checkpoint
                .filter(|(_peer_id, info)| info.height >= next)
                // Filter out any peers who we aren't connected with
                .flat_map(|(peer_id, _height)| network.peer(*peer_id))
                .map(StateSyncClient::new)
                .collect::<Vec<_>>();
            rand::seq::SliceRandom::shuffle(peers.as_mut_slice(), &mut rng);
            let peer_heights = peer_heights.clone();
            async move {
                if let Some(checkpoint) = peer_heights
                    .read()
                    .unwrap()
                    .get_checkpoint_by_sequence_number(next)
                {
                    return (Some(checkpoint.to_owned()), next, None);
                }

                // Iterate through our selected peers trying each one in turn until we're able to
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
                            continue;
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
        debug_assert!(current.sequence_number().saturating_add(1) == next);

        // Verify the checkpoint
        let checkpoint = {
            let checkpoint = maybe_checkpoint
                .ok_or_else(|| anyhow::anyhow!("no peers were able to help sync"))?;
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

        debug!(sequence_number = ?checkpoint.sequence_number(), "verified checkpoint summary");
        SystemTime::now()
            .duration_since(checkpoint.timestamp())
            .map(|latency| metrics.report_checkpoint_summary_age(latency))
            .tap_err(|err| warn!("unable to compute checkpoint age: {}", err))
            .ok();

        current = checkpoint.clone();
        // Insert the newly verified checkpoint into our store, which will bump our highest
        // verified checkpoint watermark as well.
        store
            .insert_checkpoint(checkpoint.clone())
            .expect("store operation should not fail");
        metrics.set_highest_verified_checkpoint(*checkpoint.sequence_number());
    }

    peer_heights
        .write()
        .unwrap()
        .cleanup_old_checkpoints(*checkpoint.sequence_number());

    Ok(())
}

fn verify_checkpoint<S>(
    current: &VerifiedCheckpoint,
    store: S,
    checkpoint: Checkpoint,
) -> Result<VerifiedCheckpoint, Checkpoint>
where
    S: WriteStore,
    <S as ReadStore>::Error: std::error::Error,
{
    assert_eq!(
        *checkpoint.sequence_number(),
        current.sequence_number().saturating_add(1)
    );

    if Some(*current.digest()) != checkpoint.previous_digest {
        debug!(
            current_sequence_number = current.sequence_number(),
            current_digest =% current.digest(),
            checkpoint_sequence_number = checkpoint.sequence_number(),
            checkpoint_digest =% checkpoint.digest(),
            checkpoint_previous_digest =? checkpoint.previous_digest,
            "checkpoint not on same chain"
        );
        return Err(checkpoint);
    }

    let current_epoch = current.epoch();
    if checkpoint.epoch() != current_epoch && checkpoint.epoch() != current_epoch.saturating_add(1)
    {
        debug!(
            current_epoch = current_epoch,
            checkpoint_epoch = checkpoint.epoch(),
            "cannot verify checkpoint with too high of an epoch",
        );
        return Err(checkpoint);
    }

    if checkpoint.epoch() == current_epoch.saturating_add(1)
        && current.next_epoch_committee().is_none()
    {
        debug!(
            "next checkpoint claims to be from the next epoch but the latest verified \
            checkpoint does not indicate that it is the last checkpoint of an epoch"
        );
        return Err(checkpoint);
    }

    let committee = store
        .get_committee(checkpoint.epoch())
        .expect("store operation should not fail")
        .expect("BUG: should have a committee for an epoch before we try to verify checkpoints from an epoch");

    checkpoint.verify_signature(&committee).map_err(|e| {
        debug!("error verifying checkpoint: {e}");
        checkpoint.clone()
    })?;
    Ok(VerifiedCheckpoint::new_unchecked(checkpoint))
}

async fn sync_checkpoint_contents<S>(
    network: anemo::Network,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    sender: mpsc::WeakSender<StateSyncMessage>,
    checkpoint_event_sender: broadcast::Sender<VerifiedCheckpoint>,
    metrics: Metrics,
    checkpoint_content_download_concurrency: usize,
    checkpoint_content_download_tx_concurrency: u64,
    timeout: Duration,
    mut target_sequence_channel: watch::Receiver<CheckpointSequenceNumber>,
) where
    S: WriteStore + Clone,
    <S as ReadStore>::Error: std::error::Error,
{
    let mut highest_synced = store
        .get_highest_synced_checkpoint()
        .expect("store operation should not fail");

    let mut current_sequence = highest_synced.sequence_number().saturating_add(1);
    let mut target_sequence_cursor = 0;
    let mut highest_started_network_total_transactions = highest_synced.network_total_transactions;
    let mut checkpoint_contents_tasks = FuturesOrdered::new();

    let mut tx_concurrency_remaining = checkpoint_content_download_tx_concurrency;

    loop {
        tokio::select! {
            result = target_sequence_channel.changed() => {
                match result {
                    Ok(()) => {
                        target_sequence_cursor = (*target_sequence_channel.borrow_and_update()).saturating_add(1);
                    }
                    Err(_) => {
                        // Watch channel is closed, exit loop.
                        return
                    }
                }
            },
            Some(maybe_checkpoint) = checkpoint_contents_tasks.next() => {
                match maybe_checkpoint {
                    Ok((checkpoint, num_txns)) => {
                        let _: &VerifiedCheckpoint = &checkpoint;  // type hint
                        // if this fails, there is a bug in checkpoint construction (or the chain is
                        // corrupted)
                        assert_eq!(
                            highest_synced.network_total_transactions + num_txns,
                            checkpoint.network_total_transactions
                        );
                        tx_concurrency_remaining += num_txns;

                        store
                            .update_highest_synced_checkpoint(&checkpoint)
                            .expect("store operation should not fail");
                        metrics.set_highest_synced_checkpoint(*checkpoint.sequence_number());
                        // We don't care if no one is listening as this is a broadcast channel
                        let _ = checkpoint_event_sender.send(checkpoint.clone());
                        highest_synced = checkpoint;

                    }
                    Err(checkpoint) => {
                        let _: &VerifiedCheckpoint = &checkpoint;  // type hint
                        debug!("unable to sync contents of checkpoint {}", checkpoint.sequence_number());
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
                .expect("store operation should not fail")
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

async fn sync_one_checkpoint_contents<S>(
    network: anemo::Network,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    timeout: Duration,
    checkpoint: VerifiedCheckpoint,
) -> Result<(VerifiedCheckpoint, u64), VerifiedCheckpoint>
where
    S: WriteStore + Clone,
    <S as ReadStore>::Error: std::error::Error,
{
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_entropy();
    // get a list of peers that can help
    let mut peers = peer_heights
        .read()
        .unwrap()
        .peers_on_same_chain()
        // Filter out any peers who can't help with this particular checkpoint
        .filter(|(_peer_id, info)| info.height >= *checkpoint.sequence_number())
        // Filter out any peers who we aren't connected with
        .flat_map(|(peer_id, _height)| network.peer(*peer_id))
        .map(StateSyncClient::new)
        .collect::<Vec<_>>();
    rand::seq::SliceRandom::shuffle(peers.as_mut_slice(), &mut rng);

    let Some(contents) = get_full_checkpoint_contents(&mut peers, &store, &checkpoint, timeout).await else {
        // Delay completion in case of error so we don't hammer the network with retries.
        tokio::time::sleep(Duration::from_secs(10)).await;
        return Err(checkpoint);
    };

    let num_txns = contents.size() as u64;

    Ok((checkpoint, num_txns))
}

async fn get_full_checkpoint_contents<S>(
    peers: &mut [StateSyncClient<anemo::Peer>],
    store: S,
    checkpoint: &VerifiedCheckpoint,
    timeout: Duration,
) -> Option<FullCheckpointContents>
where
    S: WriteStore,
    <S as ReadStore>::Error: std::error::Error,
{
    let digest = checkpoint.content_digest;
    if let Some(contents) = store
        .get_full_checkpoint_contents_by_sequence_number(*checkpoint.sequence_number())
        .expect("store operation should not fail")
        .or_else(|| {
            store
                .get_full_checkpoint_contents(&digest)
                .expect("store operation should not fail")
        })
    {
        return Some(contents);
    }

    // Iterate through our selected peers trying each one in turn until we're able to
    // successfully get the target checkpoint
    for peer in peers.iter_mut() {
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

    None
}
