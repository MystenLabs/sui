// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::AuthorityState,
    authority_active::gossip::{DigestHandler, Follower, GossipMetrics},
    authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
};
use async_trait::async_trait;

use tokio_stream::{Stream, StreamExt};

use std::collections::{hash_map, BTreeSet, HashMap};
use sui_storage::node_sync_store::NodeSyncStore;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, TransactionDigest, TransactionEffectsDigest},
    committee::{Committee, StakeUnit},
    error::{SuiError, SuiResult},
    messages::{CertifiedTransaction, SignedTransactionEffects, TransactionInfoResponse},
    messages_checkpoint::CheckpointContents,
};

use std::ops::Deref;
use std::sync::{Arc, Mutex};

use futures::{future::BoxFuture, stream::FuturesOrdered, FutureExt};

use tap::TapFallible;

use tokio::sync::{broadcast, mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;

use tracing::{debug, error, trace, warn};

const NODE_SYNC_QUEUE_LEN: usize = 500;

// Process up to 20 digests concurrently.
const MAX_NODE_SYNC_CONCURRENCY: usize = 20;

/// EffectsStakeMap tracks which effects digests have been attested by a quorum of validators and
/// are thus final.
struct EffectsStakeMap {
    /// Keep track of how much stake has voted for a given effects digest
    /// any entry in this map with >2f+1 stake can be sequenced locally and
    /// removed from the map.
    effects_stake_map: HashMap<TransactionEffectsDigest, StakeUnit>,
    /// Keep track of stake votes per validator - needed to double check the total stored in
    /// effects_stake_map, which can otherwise be corrupted by byzantine double-voting.
    effects_vote_map: HashMap<TransactionEffectsDigest, HashMap<AuthorityName, StakeUnit>>,
}

impl EffectsStakeMap {
    pub fn new() -> Self {
        Self {
            effects_stake_map: HashMap::new(),
            effects_vote_map: HashMap::new(),
        }
    }

    // Get the set of authorities who voted for a digest.
    pub fn voters(&self, digest: &TransactionEffectsDigest) -> BTreeSet<AuthorityName> {
        self.effects_vote_map
            .get(digest)
            .unwrap_or(&HashMap::new())
            .keys()
            .cloned()
            .collect()
    }

    /// Note that a given effects digest has been attested by a validator, and return true if the
    /// stake that has attested that effects digest has exceeded the quorum threshold.
    pub fn note_effects_digest(
        &mut self,
        source: &AuthorityName,
        stake: StakeUnit,
        quorum_threshold: StakeUnit,
        effects_digest: &TransactionEffectsDigest,
    ) -> bool {
        let validator_map = self
            .effects_vote_map
            .entry(*effects_digest)
            .or_insert_with(HashMap::new);

        let vote_entry = validator_map.entry(*source);

        let cur_stake = if let hash_map::Entry::Occupied(_) = &vote_entry {
            // TODO: report byzantine authority suspciion
            warn!(peer = ?source, ?effects_digest,
                "ByzantineAuthoritySuspicion: peer double-voted for effects digest");
            self.effects_stake_map.entry(*effects_digest).or_insert(0)
        } else {
            vote_entry.or_insert(stake);

            self.effects_stake_map
                .entry(*effects_digest)
                .and_modify(|cur| *cur += stake)
                .or_insert(stake)
        };

        let is_final = *cur_stake >= quorum_threshold;
        if !is_final {
            trace!(
                ?effects_digest,
                "tx cert/effects not yet final: {} < {}",
                *cur_stake,
                quorum_threshold
            );
        }
        is_final
    }

    pub fn forget_effects(&mut self, digests: &TransactionEffectsDigest) {
        self.effects_stake_map.remove(digests);
        self.effects_vote_map.remove(digests);
    }
}

/// Waiter is used to single-shot concurrent requests and wait for dependencies to finish.
struct Waiter<Key, ResultT> {
    waiters: Mutex<HashMap<Key, broadcast::Sender<ResultT>>>,
}

impl<Key, ResultT> Waiter<Key, ResultT>
where
    Key: std::hash::Hash + Eq + Clone + std::fmt::Debug,
    ResultT: Clone,
{
    fn new() -> Self {
        Self {
            waiters: Mutex::new(HashMap::new()),
        }
    }

    /// Returns (true, rx) if there are no other waiters yet, or else (false, rx).
    /// All rxes can be woken by calling notify(key, result)
    async fn wait(&self, key: &Key) -> (bool, broadcast::Receiver<ResultT>) {
        let waiters = &mut self.waiters.lock().unwrap();
        let entry = waiters.entry(key.clone());

        match entry {
            hash_map::Entry::Occupied(e) => (false, e.get().subscribe()),
            hash_map::Entry::Vacant(e) => {
                let (tx, rx) = broadcast::channel(1);
                e.insert(tx);
                (true, rx)
            }
        }
    }

    async fn notify(&self, key: &Key, res: ResultT) -> SuiResult {
        if let Some(tx) = self.waiters.lock().unwrap().remove(key) {
            tx.send(res).map_err(|_| SuiError::GenericAuthorityError {
                error: format!("couldn't notify waiters for key {:?}", key),
            })?;
        } else {
            trace!("no pending waiters");
        }
        Ok(())
    }
}

struct DigestsMessage {
    sync_arg: SyncArg,
    tx: Option<oneshot::Sender<SuiResult>>,
}

impl DigestsMessage {
    fn new_for_ckpt(digests: &ExecutionDigests, tx: oneshot::Sender<SuiResult>) -> Self {
        Self {
            sync_arg: SyncArg::Checkpoint(*digests),
            tx: Some(tx),
        }
    }

    fn new_for_exec_driver(digest: &TransactionDigest, tx: oneshot::Sender<SuiResult>) -> Self {
        Self {
            sync_arg: SyncArg::ExecDriver(*digest),
            tx: Some(tx),
        }
    }

    fn new(
        digests: &ExecutionDigests,
        peer: AuthorityName,
        tx: oneshot::Sender<SuiResult>,
    ) -> Self {
        Self {
            sync_arg: SyncArg::Follow(peer, *digests),
            tx: Some(tx),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum SyncArg {
    /// In follow mode, wait for 2f+1 votes for a tx before executing
    Follow(AuthorityName, ExecutionDigests),

    /// In checkpoint mode, all txes are known to be final.
    Checkpoint(ExecutionDigests),

    /// Used by the execution driver to execute pending certs. No effects digest is provided,
    /// because this mode is used on validators only, who must compute the effects digest
    /// themselves - they cannot trust some other validator's version of the effects because that
    /// validator may be byzantine.
    ExecDriver(TransactionDigest),
}

impl SyncArg {
    fn transaction_digest(&self) -> &TransactionDigest {
        self.digests().0
    }

    fn digests(&self) -> (&TransactionDigest, Option<&TransactionEffectsDigest>) {
        match self {
            SyncArg::Checkpoint(ExecutionDigests {
                transaction,
                effects,
            })
            | SyncArg::Follow(
                _,
                ExecutionDigests {
                    transaction,
                    effects,
                },
            ) => (transaction, Some(effects)),
            SyncArg::ExecDriver(digest) => (digest, None),
        }
    }
}

#[derive(Debug, Clone)]
enum DownloadRequest {
    Node(ExecutionDigests),
    Validator(TransactionDigest),
}

impl DownloadRequest {
    fn transaction_digest(&self) -> &TransactionDigest {
        match self {
            Self::Node(d) => &d.transaction,
            Self::Validator(d) => d,
        }
    }
}

/// NodeSyncState is shared by any number of NodeSyncHandle's, and receives DigestsMessage
/// messages from those handlers, waits for finality of TXes, and then downloads and applies those
/// TXes locally.
pub struct NodeSyncState<A> {
    committee: Arc<Committee>,
    effects_stake: Mutex<EffectsStakeMap>,
    state: Arc<AuthorityState>,
    node_sync_store: Arc<NodeSyncStore>,
    aggregator: Arc<AuthorityAggregator<A>>,

    // Used to single-shot multiple concurrent downloads.
    pending_downloads: Arc<Waiter<TransactionDigest, SuiResult>>,

    // Used to wait for parent transactions to be applied locally
    pending_txes: Waiter<TransactionDigest, ()>,

    // Channels for enqueuing DigestMessage requests.
    sender: mpsc::Sender<DigestsMessage>,
    receiver: Arc<tokio::sync::Mutex<mpsc::Receiver<DigestsMessage>>>,

    // Gossip Metrics
    metrics: GossipMetrics,
}

impl<A> NodeSyncState<A> {
    pub fn new(
        state: Arc<AuthorityState>,
        aggregator: Arc<AuthorityAggregator<A>>,
        node_sync_store: Arc<NodeSyncStore>,
        metrics: GossipMetrics,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(NODE_SYNC_QUEUE_LEN);
        let committee = state.committee.load().deref().clone();
        Self {
            committee,
            effects_stake: Mutex::new(EffectsStakeMap::new()),
            state,
            aggregator,
            node_sync_store,
            pending_downloads: Arc::new(Waiter::new()),
            pending_txes: Waiter::new(),
            sender,
            receiver: Arc::new(tokio::sync::Mutex::new(receiver)),
            metrics,
        }
    }
}

impl<A> NodeSyncState<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    fn start(self: Arc<Self>) -> (JoinHandle<()>, mpsc::Sender<DigestsMessage>) {
        let sender = self.sender.clone();
        let state = self;

        let mut receiver = match state.receiver.clone().try_lock_owned() {
            Err(_) => {
                // There's no reason this should ever happen, but if it does a bug - it would be
                // better to change NodeSyncState::start() to return SuiResult but that turns out
                // to be very awkward. Instead we just return the clone of the sender and start a
                // new task that will exit at the same time as the original task.
                error!(
                    "Duplicate call to NodeSyncState::start() - caller will block until \
                    previous task terminates (probably never)"
                );

                return (
                    tokio::spawn(async move {
                        state.receiver.lock().await;
                    }),
                    sender,
                );
            }
            Ok(r) => r,
        };

        (
            tokio::spawn(async move { state.handle_messages(&mut receiver).await }),
            sender,
        )
    }

    async fn handle_messages(self: Arc<Self>, receiver: &mut mpsc::Receiver<DigestsMessage>) {
        // this pattern for limiting concurrency is from
        // https://github.com/tokio-rs/tokio/discussions/2648
        let limit = Arc::new(Semaphore::new(MAX_NODE_SYNC_CONCURRENCY));

        while let Some(DigestsMessage { sync_arg, tx }) = receiver.recv().await {
            let state = self.clone();
            let limit = limit.clone();

            // hold semaphore permit until task completes. unwrap ok because we never close
            // the semaphore in this context.
            let permit = limit.acquire_owned().await.unwrap();

            tokio::spawn(async move {
                let res = state.process_digest(sync_arg, permit).await;
                if let Err(error) = &res {
                    error!(?sync_arg, "process_digest failed: {}", error);
                }

                // Notify waiters even if tx failed, to avoid leaking resources.
                let digest = sync_arg.transaction_digest();
                trace!(?digest, "notifying waiters");
                state
                    .pending_txes
                    .notify(digest, ())
                    .await
                    .tap_err(|e| debug!(?digest, "{}", e))
                    .ok();

                if let Some(tx) = tx {
                    // Send status back to follower so that it knows whether to advance
                    // the watermark.
                    if tx.send(res).is_err() {
                        // This will happen any time the follower times out and restarts, but
                        // that's ok - the follower won't have marked this digest as processed so it
                        // will be retried.
                        debug!(
                            ?sync_arg,
                            "could not send process_digest response to caller",
                        );
                    }
                }
            });
        }
    }

    async fn process_exec_driver_digest(
        &self,
        permit: OwnedSemaphorePermit,
        digest: &TransactionDigest,
    ) -> SuiResult {
        trace!(?digest, "validator pending execution requested");

        let cert = match self.state.database.read_certificate(digest)? {
            Some(cert) => cert,
            None => {
                let (cert, _) = self
                    .download_cert_and_effects(None, &DownloadRequest::Validator(*digest))
                    .await?;
                cert
            }
        };

        match self.state.handle_certificate(cert.clone()).await {
            Ok(_) => Ok(()),
            Err(SuiError::ObjectErrors { .. }) => {
                debug!(?digest, "cert execution failed due to missing parents");

                let effects = self.aggregator.execute_cert_to_true_effects(&cert).await?;
                let parents = &effects.effects().dependencies;

                // Must release permit before enqueuing new work to prevent deadlock.
                std::mem::drop(permit);

                debug!(?parents, "attempting to execute parents");

                let handle =
                    NodeSyncHandle::new_from_sender(self.sender.clone(), self.metrics.clone());
                let results = handle
                    .handle_execution_request(parents.iter().cloned())
                    .await?;

                let errors: Vec<_> = results.filter_map(|r| r.err()).collect().await;

                if errors.is_empty() {
                    // Parents have been executed, so this should now succeed.
                    debug!(?digest, "parents executed, re-attempting cert");
                    self.state.handle_certificate(cert.clone()).await?;
                    Ok(())
                } else {
                    Err(SuiError::ExecutionDriverError {
                        digest: *digest,
                        msg: "Could not execute all parent certificates".into(),
                        errors,
                    })
                }
            }
            Err(e) => Err(e),
        }
    }

    async fn process_digest(&self, arg: SyncArg, permit: OwnedSemaphorePermit) -> SuiResult {
        trace!(?arg, "process_digest");

        let (digest, _effects_digest) = arg.digests();

        // check if the tx is already locally final
        if self.state.database.effects_exists(digest)? {
            return Ok(());
        }

        // TODO: We could kick off the cert download now, as an optimization. For simplicity
        // we wait until we have the final effects digest and download them both at once, after the
        // is_final check. We can't download the effects yet because a SignedEffects is signed
        // only by the originating validator and so can't be trusted until we have seen at least
        // f+1 identical effects digests.
        //
        // There is a further optimization which is that we could start downloading the effects
        // earlier than we do as well, after f+1 instead of 2f+1. Then when we reach 2f+1 we might
        // already have everything stored locally.
        //
        // These optimizations may well be worth it at some point if we are trying to get latency
        // down.
        let (digests, authorities_with_cert) = match arg {
            SyncArg::ExecDriver(digest) => {
                return self.process_exec_driver_digest(permit, &digest).await
            }
            SyncArg::Follow(peer, digests) => {
                // Check if the tx is final.
                let stake = self.committee.weight(&peer);
                let quorum_threshold = self.committee.quorum_threshold();

                let is_final = self.effects_stake.lock().unwrap().note_effects_digest(
                    &peer,
                    stake,
                    quorum_threshold,
                    &digests.effects,
                );

                if !is_final {
                    // we won't be downloading anything, so release the permit
                    std::mem::drop(permit);

                    // wait until the tx becomes final before returning, so that the follower doesn't mark
                    // this tx as finished prematurely.
                    let _timer = self.metrics.wait_for_finality_latency_sec.start_timer();
                    let (_, mut rx) = self.pending_txes.wait(&digests.transaction).await;
                    let result = rx
                        .recv()
                        .await
                        .map_err(|e| SuiError::GenericAuthorityError {
                            error: format!("{:?}", e),
                        });
                    return result;
                }

                debug!(?digests, ?peer, "digests are now final");

                (
                    digests,
                    Some(self.effects_stake.lock().unwrap().voters(&digests.effects)),
                )
            }
            SyncArg::Checkpoint(digests) => {
                trace!(
                    ?digests,
                    "skipping finality check, syncing from checkpoint."
                );
                (digests, None)
            }
        };

        // Download the cert and effects - either finality has been establish (above), or
        // we are a validator.
        let (cert, effects) = self
            .download_cert_and_effects(authorities_with_cert, &DownloadRequest::Node(digests))
            .await?;

        // Node sync request arrive in causal order via the follower API, so it is always safe to
        // assume that the parents of this cert have already been enqueued.
        self.wait_for_parents(permit, &digests.transaction, &effects)
            .await?;

        self.state
            .handle_node_sync_certificate(cert, effects.clone())
            .await?;

        // Garbage collect data for this tx.
        self.effects_stake
            .lock()
            .unwrap()
            .forget_effects(effects.digest());
        self.node_sync_store.delete_cert_and_effects(digest)?;

        Ok(())
    }

    async fn wait_for_parents(
        &self,
        permit: OwnedSemaphorePermit,
        digest: &TransactionDigest,
        effects: &SignedTransactionEffects,
    ) -> SuiResult {
        // Must drop the permit before waiting to avoid deadlock.
        std::mem::drop(permit);

        for parent in effects.effects().dependencies.iter() {
            let (_, mut rx) = self.pending_txes.wait(parent).await;

            if self.state.database.effects_exists(parent)? {
                continue;
            }

            trace!(?parent, ?digest, "waiting for parent");
            // Since we no longer hold the semaphore permit, can be sure that our parent will be
            // able to start.
            rx.recv()
                .await
                .map_err(|e| SuiError::GenericAuthorityError {
                    error: format!("{:?}", e),
                })?;
        }

        if cfg!(debug_assertions) {
            for parent in effects.effects().dependencies.iter() {
                debug_assert!(self.state.database.effects_exists(parent).unwrap());
            }
        }

        Ok(())
    }

    // Download the certificate and effects specified in digests.
    // TODO: In checkpoint mode, we don't need to download a cert, a transaction will do.
    // Transactions are not currently persisted anywhere, however (validators delete them eagerly).
    async fn download_cert_and_effects(
        &self,
        authorities_with_cert: Option<BTreeSet<AuthorityName>>,
        req: &DownloadRequest,
    ) -> SuiResult<(CertifiedTransaction, SignedTransactionEffects)> {
        let tx_digest = *req.transaction_digest();
        if let Some(c) = self.node_sync_store.get_cert_and_effects(&tx_digest)? {
            return Ok(c);
        }
        let pending_downloads = self.pending_downloads.clone();
        let (first, mut rx) = pending_downloads.wait(&tx_digest).await;
        // Only start the download if there are no other concurrent downloads.
        if first {
            let aggregator = self.aggregator.clone();
            let node_sync_store = self.node_sync_store.clone();
            let req = req.clone();
            let metrics = self.metrics.clone();
            tokio::task::spawn(async move {
                let _ = pending_downloads
                    .notify(
                        &tx_digest,
                        Self::download_impl(
                            authorities_with_cert,
                            aggregator,
                            &req,
                            node_sync_store,
                            metrics,
                        )
                        .await,
                    )
                    .await;
            });
        }

        rx.recv()
            .await
            .map_err(|e| SuiError::GenericAuthorityError {
                error: format!("{:?}", e),
            })??;

        self.node_sync_store
            .get_cert_and_effects(&tx_digest)?
            .ok_or_else(|| SuiError::GenericAuthorityError {
                error: format!(
                    "cert/effects for {:?} should have been in the node_sync_store",
                    tx_digest
                ),
            })
    }

    async fn download_impl(
        authorities: Option<BTreeSet<AuthorityName>>,
        aggregator: Arc<AuthorityAggregator<A>>,
        req: &DownloadRequest,
        node_sync_store: Arc<NodeSyncStore>,
        metrics: GossipMetrics,
    ) -> SuiResult {
        let (cert, effects) = match req {
            DownloadRequest::Node(digests) => {
                metrics.total_attempts_cert_downloads.inc();
                let resp = aggregator
                    .handle_transaction_and_effects_info_request(
                        digests,
                        authorities.as_ref(),
                        None,
                    )
                    .await?;
                metrics.total_successful_attempts_cert_downloads.inc();
                resp
            }
            DownloadRequest::Validator(digest) => {
                let resp = aggregator.handle_cert_info_request(digest, None).await?;
                match resp {
                    TransactionInfoResponse {
                        certified_transaction: Some(cert),
                        signed_effects: Some(effects),
                        ..
                    } => (cert, effects),
                    _ => return Err(SuiError::TransactionNotFound { digest: *digest }),
                }
            }
        };

        node_sync_store.store_cert_and_effects(req.transaction_digest(), &(cert, effects))?;
        Ok(())
    }
}

/// A cloneable handle that can send messages to a NodeSyncState
#[derive(Clone)]
pub struct NodeSyncHandle {
    sender: mpsc::Sender<DigestsMessage>,
    metrics: GossipMetrics,
}

impl NodeSyncHandle {
    pub fn new<A>(sync_state: Arc<NodeSyncState<A>>, metrics: GossipMetrics) -> Self
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let (_handle, sender) = sync_state.start();

        Self { sender, metrics }
    }

    fn new_from_sender(sender: mpsc::Sender<DigestsMessage>, metrics: GossipMetrics) -> Self {
        Self { sender, metrics }
    }

    fn map_rx(rx: oneshot::Receiver<SuiResult>) -> BoxFuture<'static, SuiResult> {
        Box::pin(rx.map(|res| {
            let res = res.map_err(|e| SuiError::GenericAuthorityError {
                error: e.to_string(),
            });
            match res {
                Ok(r) => r,
                Err(e) => Err(e),
            }
        }))
    }

    async fn send_msg_with_tx(
        sender: mpsc::Sender<DigestsMessage>,
        msg: DigestsMessage,
    ) -> SuiResult {
        sender
            .send(msg)
            .await
            .map_err(|e| SuiError::GenericAuthorityError {
                error: e.to_string(),
            })
    }

    pub async fn sync_checkpoint(
        &self,
        checkpoint_contents: &CheckpointContents,
    ) -> SuiResult<impl Stream<Item = SuiResult>> {
        let mut futures = FuturesOrdered::new();
        for digests in checkpoint_contents.iter() {
            let (tx, rx) = oneshot::channel();
            let msg = DigestsMessage::new_for_ckpt(digests, tx);
            Self::send_msg_with_tx(self.sender.clone(), msg).await?;
            futures.push_back(Self::map_rx(rx));
        }

        Ok(futures)
    }

    pub async fn handle_execution_request(
        &self,
        digests: impl Iterator<Item = TransactionDigest>,
    ) -> SuiResult<impl Stream<Item = SuiResult>> {
        let mut futures = FuturesOrdered::new();
        for digest in digests {
            let (tx, rx) = oneshot::channel();
            let msg = DigestsMessage::new_for_exec_driver(&digest, tx);
            Self::send_msg_with_tx(self.sender.clone(), msg).await?;
            futures.push_back(Self::map_rx(rx));
        }

        Ok(futures)
    }
}

#[async_trait]
impl<A> DigestHandler<A> for NodeSyncHandle
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    type DigestResult = BoxFuture<'static, SuiResult>;

    async fn handle_digest(
        &self,
        follower: &Follower<A>,
        digests: ExecutionDigests,
    ) -> SuiResult<Self::DigestResult> {
        let (tx, rx) = oneshot::channel();
        let sender = self.sender.clone();
        Self::send_msg_with_tx(
            sender,
            DigestsMessage::new(&digests, follower.peer_name, tx),
        )
        .await?;
        Ok(Self::map_rx(rx))
    }

    fn get_metrics(&self) -> &GossipMetrics {
        &self.metrics
    }
}

#[cfg(test)]
mod tests {
    // Note: this code is tested end-to-end in full_node_tests.rs

    use narwhal_crypto::traits::KeyPair;
    use sui_types::{
        base_types::{AuthorityName, TransactionEffectsDigest},
        crypto::{get_key_pair, AuthorityKeyPair},
    };

    use super::EffectsStakeMap;

    fn random_authority_name() -> AuthorityName {
        let key: (_, AuthorityKeyPair) = get_key_pair();
        key.1.public().into()
    }

    #[test]
    fn test_effects_stake() {
        let mut map = EffectsStakeMap::new();

        let threshold = 3;

        let byzantine = random_authority_name();
        let validator2 = random_authority_name();
        let validator3 = random_authority_name();

        let digests = TransactionEffectsDigest::random();

        assert!(!map.note_effects_digest(&byzantine, 1, threshold, &digests));
        assert!(!map.note_effects_digest(&validator2, 1, threshold, &digests));

        // double voting is rejected
        assert!(!map.note_effects_digest(&byzantine, 1, threshold, &digests));

        // final vote pushes us over.
        assert!(map.note_effects_digest(&validator3, 1, threshold, &digests));

        // double vote doesn't result in false if we already exceeded threshold.
        assert!(map.note_effects_digest(&byzantine, 1, threshold, &digests));
    }
}
