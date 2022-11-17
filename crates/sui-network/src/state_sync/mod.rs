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
//! The [PeerHeights] struct is used to track the highest_synced_checkpoint watermark for all of
//! our peers.
//!
//! When a new checkpoint is discovered, and we've determined that it is higher than our
//! highest_verified_checkpoint, then StateSync will kick off a task to synchronize and verify all
//! checkpoints between our highest_synced_checkpoint and the newly discovered checkpoint. This
//! process is done by querying one of our peers for the checkpoints we're missing (using the
//! [PeerHeights] struct as a way to intelligently select which peers have the data available for
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

// TODO
// * When querying a peer make sure that we're sending to peers that are on the same "network" as
// us. this means verifying their genesis or something else

use anemo::{rpc::Status, types::PeerEvent, PeerId, Request, Response, Result};
use anyhow::anyhow;
use futures::{FutureExt, StreamExt};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use sui_config::p2p::StateSyncConfig;
use sui_types::{
    base_types::ExecutionDigests,
    message_envelope::Message,
    messages_checkpoint::{
        CertifiedCheckpointSummary as Checkpoint, CheckpointContents, CheckpointContentsDigest,
        CheckpointDigest, CheckpointSequenceNumber, VerifiedCheckpoint,
    },
    storage::ReadStore,
    storage::WriteStore,
};
use tap::{Pipe, TapFallible, TapOptional};
use tokio::{
    sync::{broadcast, mpsc},
    task::{AbortHandle, JoinSet},
};
use tracing::{debug, info, trace, warn};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

mod generated {
    include!(concat!(env!("OUT_DIR"), "/sui.StateSync.rs"));
}
mod builder;
mod server;
#[cfg(test)]
mod tests;

pub use builder::{Builder, UnstartedStateSync};
pub use generated::{
    state_sync_client::StateSyncClient,
    state_sync_server::{StateSync, StateSyncServer},
};
pub use server::GetCheckpointSummaryRequest;

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
    ///
    /// Today we don't have the concept of a "genesis checkpoint" so when a node starts up with an
    /// empty db it won't have any checkpoints, the None case indicates this. If a node shows up in
    /// this map then they support state-sync.
    heights: HashMap<PeerId, Option<CheckpointSequenceNumber>>,
    unprocessed_checkpoints: HashMap<CheckpointDigest, Checkpoint>,
    sequence_number_to_digest: HashMap<CheckpointSequenceNumber, CheckpointDigest>,
}

impl PeerHeights {
    pub fn highest_known_checkpoint(&self) -> Option<&Checkpoint> {
        self.heights
            .values()
            .max()
            .and_then(Clone::clone)
            .and_then(|s| self.sequence_number_to_digest.get(&s))
            .and_then(|digest| self.unprocessed_checkpoints.get(digest))
    }

    pub fn update_peer_height(&mut self, peer_id: PeerId, checkpoint: Option<Checkpoint>) {
        use std::collections::hash_map::Entry;

        let latest = checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.sequence_number());

        match self.heights.entry(peer_id) {
            Entry::Occupied(mut entry) => {
                if latest > *entry.get() {
                    entry.insert(latest);
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(latest);
            }
        }

        if let Some(checkpoint) = checkpoint {
            self.insert_checkpoint(checkpoint);
        }
    }

    pub fn cleanup_old_checkpoints(&mut self, sequence_number: CheckpointSequenceNumber) {
        self.unprocessed_checkpoints
            .retain(|_digest, checkpoint| checkpoint.sequence_number() > sequence_number);
        self.sequence_number_to_digest
            .retain(|&s, _digest| s > sequence_number);
    }

    pub fn insert_checkpoint(&mut self, checkpoint: Checkpoint) {
        let digest = checkpoint.digest();
        let sequence_number = checkpoint.sequence_number();
        self.unprocessed_checkpoints.insert(digest, checkpoint);
        self.sequence_number_to_digest
            .insert(sequence_number, digest);
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

    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    checkpoint_event_sender: broadcast::Sender<VerifiedCheckpoint>,
    network: anemo::Network,
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
            let (subscriber, peers) = self.network.subscribe();
            for peer_id in peers {
                self.spawn_get_latest_from_peer(peer_id);
            }
            subscriber
        };

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
                    task_result.unwrap();

                    if matches!(&self.sync_checkpoint_contents_task, Some(t) if t.is_finished()) {
                        self.sync_checkpoint_contents_task = None;
                    }

                    if matches!(&self.sync_checkpoint_summaries_task, Some(t) if t.is_finished()) {
                        self.sync_checkpoint_summaries_task = None;
                    }
                },
            }

            self.maybe_start_checkpoint_summary_sync_task();
            self.maybe_start_checkpoint_contents_sync_task();
        }

        info!("State-Synchronizer ended");
    }

    fn handle_message(&mut self, message: StateSyncMessage) {
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
            if latest_checkpoint.as_ref().map(|x| x.sequence_number())
                >= Some(checkpoint.sequence_number())
            {
                return;
            }

            let next_sequence_number = latest_checkpoint
                .as_ref()
                .map(|x| x.sequence_number().saturating_add(1))
                .unwrap_or(0);
            let previous_digest = latest_checkpoint.map(|x| x.digest());
            (next_sequence_number, previous_digest)
        };

        // If this is exactly the next checkpoint then insert it and then notify our peers
        if checkpoint.sequence_number() == next_sequence_number
            && checkpoint.previous_digest() == previous_digest
        {
            let checkpoint = *checkpoint;

            // Check invariant that consensus must only send state-sync fully synced checkpoints
            #[cfg(debug_assertions)]
            {
                let contents = self
                    .store
                    .get_checkpoint_contents(&checkpoint.content_digest())
                    .expect("store operation should not fail")
                    .unwrap();
                for digests in contents.into_inner() {
                    debug_assert!(self
                        .store
                        .get_transaction(&digests.transaction)
                        .expect("store operation should not fail")
                        .is_some());
                    debug_assert!(self
                        .store
                        .get_transaction_effects(&digests.effects)
                        .expect("store operation should not fail")
                        .is_some());
                }
            }

            self.store
                .insert_checkpoint(checkpoint.clone())
                .expect("store operation should not fail");
            self.store
                .update_highest_synced_checkpoint(&checkpoint)
                .expect("store operation should not fail");

            // We don't care if no one is listening as this is a broadcast channel
            let _ = self.checkpoint_event_sender.send(checkpoint.clone());

            self.spawn_notify_peers_of_checkpoint(checkpoint);
        } else {
            // Otherwise stick it with the other unprocessed checkpoints and we can try to sync the missing
            // ones
            self.peer_heights
                .write()
                .unwrap()
                .insert_checkpoint(checkpoint.into_inner());
            warn!("Consensus gave us too new of a checkpoint");
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
                self.peer_heights.write().unwrap().heights.remove(&peer_id);
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
            let task = get_latest_from_peer(peer, self.peer_heights.clone());
            self.tasks.spawn(task);
        }
    }

    fn handle_tick(&mut self, _now: std::time::Instant) {
        let task = query_peers_for_their_latest_checkpoint(
            self.network.clone(),
            self.peer_heights.clone(),
            self.weak_sender.clone(),
        );
        self.tasks.spawn(task);
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

        if highest_processed_checkpoint.map(|x| x.sequence_number())
            < highest_known_checkpoint
                .as_ref()
                .map(|x| x.sequence_number())
        {
            // start sync job
            let task = sync_to_checkpoint(
                self.network.clone(),
                self.store.clone(),
                self.peer_heights.clone(),
                self.config.checkpoint_header_download_concurrency(),
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

    fn maybe_start_checkpoint_contents_sync_task(&mut self) {
        // Only run one sync task at a time
        if self.sync_checkpoint_contents_task.is_some() {
            return;
        }

        let highest_verified_checkpoint = self
            .store
            .get_highest_verified_checkpoint()
            .expect("store operation should not fail");
        let highest_synced_checkpoint = self
            .store
            .get_highest_synced_checkpoint()
            .expect("store operation should not fail");

        if highest_verified_checkpoint
            .as_ref()
            .map(|x| x.sequence_number())
            > highest_synced_checkpoint
                .as_ref()
                .map(|x| x.sequence_number())
        {
            let task = sync_checkpoint_contents(
                self.network.clone(),
                self.store.clone(),
                self.peer_heights.clone(),
                self.weak_sender.clone(),
                self.checkpoint_event_sender.clone(),
                self.config.transaction_download_concurrency(),
                // The if condition should ensure that this is Some
                highest_verified_checkpoint.unwrap(),
            );

            let task_handle = self.tasks.spawn(task);
            self.sync_checkpoint_contents_task = Some(task_handle);
        }
    }

    fn spawn_notify_peers_of_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        let task =
            notify_peers_of_checkpoint(self.network.clone(), self.peer_heights.clone(), checkpoint);
        self.tasks.spawn(task);
    }
}

async fn notify_peers_of_checkpoint(
    network: anemo::Network,
    peer_heights: Arc<RwLock<PeerHeights>>,
    checkpoint: VerifiedCheckpoint,
) {
    let futs = peer_heights
        .read()
        .unwrap()
        .heights
        .iter()
        // Filter out any peers who we know already have a checkpoint higher than this one
        .filter(|(_peer_id, &height)| Some(checkpoint.sequence_number()) > height)
        .map(|(peer_id, _height)| peer_id)
        // Filter out any peers who we aren't connected with
        .flat_map(|peer_id| network.peer(*peer_id))
        .map(StateSyncClient::new)
        .map(|mut client| {
            let request = Request::new(checkpoint.inner().clone()).with_timeout(DEFAULT_TIMEOUT);
            async move { client.push_checkpoint_summary(request).await }
        })
        .collect::<Vec<_>>();
    futures::future::join_all(futs).await;
}

async fn get_latest_from_peer(peer: anemo::Peer, peer_heights: Arc<RwLock<PeerHeights>>) {
    let peer_id = peer.peer_id();
    let request = Request::new(GetCheckpointSummaryRequest::Latest).with_timeout(DEFAULT_TIMEOUT);
    let response = StateSyncClient::new(peer)
        .get_checkpoint_summary(request)
        .await
        .map(Response::into_inner);
    update_peer_height(&peer_heights, peer_id, &response);
}

fn update_peer_height(
    peer_heights: &RwLock<PeerHeights>,
    peer_id: PeerId,
    response: &Result<Option<Checkpoint>, Status>,
) {
    match response {
        Ok(latest) => {
            peer_heights
                .write()
                .unwrap()
                .update_peer_height(peer_id, latest.clone());
        }
        Err(status) => {
            trace!("get_latest_checkpoint_summary request failed: {status:?}");
            peer_heights.write().unwrap().heights.remove(&peer_id);
        }
    }
}

async fn query_peers_for_their_latest_checkpoint(
    network: anemo::Network,
    peer_heights: Arc<RwLock<PeerHeights>>,
    sender: mpsc::WeakSender<StateSyncMessage>,
) {
    let peer_heights = &peer_heights;
    let futs = peer_heights
        .read()
        .unwrap()
        .heights
        .keys()
        // Filter out any peers who we aren't connected with
        .flat_map(|peer_id| network.peer(*peer_id))
        .map(|peer| {
            let peer_id = peer.peer_id();
            let mut client = StateSyncClient::new(peer);

            let request =
                Request::new(GetCheckpointSummaryRequest::Latest).with_timeout(DEFAULT_TIMEOUT);
            async move {
                let response = client
                    .get_checkpoint_summary(request)
                    .await
                    .map(Response::into_inner);
                update_peer_height(peer_heights, peer_id, &response);
                response
            }
        })
        .collect::<Vec<_>>();

    let checkpoints = futures::future::join_all(futs)
        .await
        .into_iter()
        .flatten()
        .flatten();

    let highest_checkpoint = checkpoints.max_by_key(|checkpoint| checkpoint.sequence_number());

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
    checkpoint_header_download_concurrency: usize,
    checkpoint: Checkpoint,
) -> Result<()>
where
    S: WriteStore,
    <S as ReadStore>::Error: std::error::Error,
{
    let mut current = store
        .get_highest_verified_checkpoint()
        .expect("store operation should not fail");
    if current.as_ref().map(|x| x.sequence_number()) >= Some(checkpoint.sequence_number()) {
        return Err(anyhow::anyhow!(
            "target checkpoint {} is older than highest verified checkpoint {}",
            checkpoint.sequence_number(),
            current.map(|x| x.sequence_number()).unwrap_or(0)
        ));
    }

    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_entropy();
    // get a list of peers that can help
    let peers = peer_heights
        .read()
        .unwrap()
        .heights
        .iter()
        // Filter out any peers who can't help
        .filter(|(_peer_id, &height)| height > current.as_ref().map(|x| x.sequence_number()))
        .map(|(&peer_id, &height)| (peer_id, height))
        .collect::<Vec<_>>();

    // range of the next sequence_numbers to fetch
    let mut request_stream = (current
        .as_ref()
        .map(|x| x.sequence_number().saturating_add(1))
        .unwrap_or(0)..=checkpoint.sequence_number())
        .map(|next| {
            let mut peers = peers
                .iter()
                // Filter out any peers who can't help with this particular checkpoint
                .filter(|(_peer_id, height)| height >= &Some(next))
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
                    return (Some(checkpoint.to_owned()), next);
                }

                // Iterate through our selected peers trying each one in turn until we're able to
                // successfully get the target checkpoint
                for mut peer in peers {
                    let request = Request::new(GetCheckpointSummaryRequest::BySequenceNumber(next))
                        .with_timeout(DEFAULT_TIMEOUT);
                    if let Some(checkpoint) = peer
                        .get_checkpoint_summary(request)
                        .await
                        .tap_err(|e| trace!("{e:?}"))
                        .ok()
                        .and_then(Response::into_inner)
                        .tap_none(|| trace!("peer unable to help sync"))
                    {
                        // peer didn't give us a checkpoint with the height that we requested
                        if checkpoint.sequence_number() != next {
                            continue;
                        }

                        // Insert in our store in the event that things fail and we need to retry
                        peer_heights
                            .write()
                            .unwrap()
                            .insert_checkpoint(checkpoint.clone());
                        return (Some(checkpoint), next);
                    }
                }

                (None, next)
            }
        })
        .pipe(futures::stream::iter)
        .buffered(checkpoint_header_download_concurrency);

    while let Some((maybe_checkpoint, next)) = request_stream.next().await {
        // Verify the checkpoint
        let checkpoint = {
            let checkpoint = maybe_checkpoint
                .ok_or_else(|| anyhow::anyhow!("no peers where able to help sync"))?;

            if checkpoint.sequence_number() != next
                || current.as_ref().map(|x| x.digest()) != checkpoint.previous_digest()
            {
                return Err(anyhow::anyhow!("detected fork"));
            }

            let current_epoch = current.as_ref().map(|x| x.epoch()).unwrap_or(0);
            if checkpoint.epoch() != current_epoch
                && checkpoint.epoch() != current_epoch.saturating_add(1)
            {
                return Err(anyhow::anyhow!(
                    "cannot verify checkpoint with too high of an epoch {}, current epoch {}",
                    checkpoint.epoch(),
                    current_epoch,
                ));
            }

            if checkpoint.epoch() == current_epoch.saturating_add(1)
                && current
                    .as_ref()
                    .and_then(|x| x.next_epoch_committee())
                    .is_none()
            {
                return Err(anyhow::anyhow!(
                    "next checkpoint claims to be from the next epoch but the latest verified \
                    checkpoint does not indicate that it is the last checkpoint of an epoch"
                ));
            }

            let committee = store
                .get_committee(checkpoint.epoch())
            .expect("store operation should not fail")
                .expect("BUG: should have a committee for an epoch before we try to verify checkpoints from an epoch");
            VerifiedCheckpoint::new(checkpoint, &committee).map_err(|(_, e)| e)?
        };

        current = Some(checkpoint.clone());
        // Insert the newly verified checkpoint into our store, which will bump our highest
        // verified checkpoint watermark as well.
        store
            .insert_checkpoint(checkpoint.clone())
            .expect("store operation should not fail");
    }

    peer_heights
        .write()
        .unwrap()
        .cleanup_old_checkpoints(checkpoint.sequence_number());

    Ok(())
}

async fn sync_checkpoint_contents<S>(
    network: anemo::Network,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    sender: mpsc::WeakSender<StateSyncMessage>,
    checkpoint_event_sender: broadcast::Sender<VerifiedCheckpoint>,
    transaction_download_concurrency: usize,
    target_checkpoint: VerifiedCheckpoint,
) where
    S: WriteStore + Clone,
    <S as ReadStore>::Error: std::error::Error,
{
    let mut highest_synced = None;

    let start = store
        .get_highest_synced_checkpoint()
        .expect("store operation should not fail")
        .map(|x| x.sequence_number().saturating_add(1))
        .unwrap_or(0);
    for checkpoint in (start..=target_checkpoint.sequence_number()).map(|next| {
        store
            .get_checkpoint_by_sequence_number(next)
            .expect("store operation should not fail")
            .expect("BUG: store should have all checkpoints older than highest_verified_checkpoint")
    }) {
        match sync_one_checkpoint_contents(
            network.clone(),
            &store,
            peer_heights.clone(),
            transaction_download_concurrency,
            checkpoint,
        )
        .await
        {
            Ok(checkpoint) => {
                store
                    .update_highest_synced_checkpoint(&checkpoint)
                    .expect("store operation should not fail");
                // We don't care if no one is listening as this is a broadcast channel
                let _ = checkpoint_event_sender.send(checkpoint.clone());
                highest_synced = Some(checkpoint);
            }
            Err(err) => {
                debug!("unable to sync contents of checkpoint: {err}");
                break;
            }
        }
    }

    // Notify event loop to notify our peers that we've synced to a new checkpoint height
    if let Some(checkpoint) = highest_synced {
        if let Some(sender) = sender.upgrade() {
            let message = StateSyncMessage::SyncedCheckpoint(Box::new(checkpoint));
            let _ = sender.send(message).await;
        }
    }
}

async fn sync_one_checkpoint_contents<S>(
    network: anemo::Network,
    store: S,
    peer_heights: Arc<RwLock<PeerHeights>>,
    transaction_download_concurrency: usize,
    checkpoint: VerifiedCheckpoint,
) -> Result<VerifiedCheckpoint>
where
    S: WriteStore + Clone,
    <S as ReadStore>::Error: std::error::Error,
{
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_entropy();
    // get a list of peers that can help
    let mut peers = peer_heights
        .read()
        .unwrap()
        .heights
        .iter()
        // Filter out any peers who can't help with this particular checkpoint
        .filter(|(_peer_id, &height)| height >= Some(checkpoint.sequence_number()))
        // Filter out any peers who we aren't connected with
        .flat_map(|(peer_id, _height)| network.peer(*peer_id))
        .map(StateSyncClient::new)
        .collect::<Vec<_>>();
    rand::seq::SliceRandom::shuffle(peers.as_mut_slice(), &mut rng);

    let Some(contents) = get_checkpoint_contents(&mut peers, &store, checkpoint.content_digest()).await else {
        return Err(anyhow!("unable to sync checkpoint contents for checkpoint {}", checkpoint.sequence_number()));
    };

    // Sync transactions and effects
    let mut stream = contents
        .into_inner()
        .into_iter()
        .map(|digests| get_transaction_and_effects(peers.clone(), store.clone(), digests))
        .pipe(futures::stream::iter)
        .buffer_unordered(transaction_download_concurrency);

    while let Some(result) = stream.next().await {
        result?;
    }

    Ok(checkpoint)
}

async fn get_checkpoint_contents<S>(
    peers: &mut [StateSyncClient<anemo::Peer>],
    store: S,
    digest: CheckpointContentsDigest,
) -> Option<CheckpointContents>
where
    S: WriteStore,
    <S as ReadStore>::Error: std::error::Error,
{
    if let Some(contents) = store
        .get_checkpoint_contents(&digest)
        .expect("store operation should not fail")
    {
        return Some(contents);
    }

    // Iterate through our selected peers trying each one in turn until we're able to
    // successfully get the target checkpoint
    for peer in peers.iter_mut() {
        let request = Request::new(digest).with_timeout(DEFAULT_TIMEOUT);
        if let Some(contents) = peer
            .get_checkpoint_contents(request)
            .await
            .tap_err(|e| trace!("{e:?}"))
            .ok()
            .and_then(Response::into_inner)
            .tap_none(|| trace!("peer unable to help sync"))
        {
            if digest == contents.digest() {
                store
                    .insert_checkpoint_contents(contents.clone())
                    .expect("store operation should not fail");
                return Some(contents);
            }
        }
    }

    None
}

async fn get_transaction_and_effects<S>(
    peers: Vec<StateSyncClient<anemo::Peer>>,
    store: S,
    digests: ExecutionDigests,
) -> Result<()>
where
    S: WriteStore,
    <S as ReadStore>::Error: std::error::Error,
{
    if let (Some(_transaction), Some(_effects)) = (
        store
            .get_transaction(&digests.transaction)
            .expect("store operation should not fail"),
        store
            .get_transaction_effects(&digests.effects)
            .expect("store operation should not fail"),
    ) {
        return Ok(());
    }

    // Iterate through our selected peers trying each one in turn until we're able to
    // successfully get the target checkpoint
    for mut peer in peers {
        let request = Request::new(digests).with_timeout(DEFAULT_TIMEOUT);
        if let Some((transaction, effects)) = peer
            .get_transaction_and_effects(request)
            .await
            .tap_err(|e| trace!("{e:?}"))
            .ok()
            .and_then(Response::into_inner)
            .tap_none(|| trace!("peer unable to help sync"))
        {
            if transaction.digest() == &digests.transaction
                && effects.digest() == digests.effects
                && effects.transaction_digest == digests.transaction
            {
                // TODO this should just be a bare Transaction type and not a TransactionCertificate
                // since Certificates are indended to be ephemeral and thrown away at the end of an
                // epoch
                store
                    .insert_transaction(sui_types::messages::VerifiedCertificate::new_unchecked(
                        transaction,
                    ))
                    .expect("store operation should not fail");
                store
                    .insert_transaction_effects(effects)
                    .expect("store operation should not fail");
                return Ok(());
            }
        }
    }

    Err(anyhow!(
        "unable to sync transaction {:?} from any of our peers",
        digests.transaction
    ))
}
