// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::AuthorityState, authority_active::gossip::GossipMetrics,
    authority_aggregator::AuthorityAggregator, authority_client::AuthorityAPI,
};

use tokio_stream::{Stream, StreamExt};

use std::collections::{hash_map, BTreeSet, HashMap};
use sui_storage::node_sync_store::NodeSyncStore;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, TransactionDigest, TransactionEffectsDigest},
    committee::Committee,
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
use tokio::time::{timeout, Duration};

use tracing::{debug, error, trace, warn};

const NODE_SYNC_QUEUE_LEN: usize = 500;

// Process up to 20 digests concurrently.
const MAX_NODE_SYNC_CONCURRENCY: usize = 20;

// All tasks die after 60 seconds if they haven't finished.
const MAX_NODE_TASK_LIFETIME: Duration = Duration::from_secs(60);

// How long to wait for parents to be processed organically before fetching/executing them
// directly.
const PARENT_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

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
    fn wait(&self, key: &Key) -> (bool, broadcast::Receiver<ResultT>) {
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
    tx: Option<oneshot::Sender<SyncResult>>,
}

impl DigestsMessage {
    fn new_for_ckpt(digests: &ExecutionDigests, tx: oneshot::Sender<SyncResult>) -> Self {
        Self {
            sync_arg: SyncArg::Checkpoint(*digests),
            tx: Some(tx),
        }
    }

    fn new_for_exec_driver(digest: &TransactionDigest, tx: oneshot::Sender<SyncResult>) -> Self {
        Self {
            sync_arg: SyncArg::ExecDriver(*digest),
            tx: Some(tx),
        }
    }

    fn new_for_parents(digest: &TransactionDigest, tx: oneshot::Sender<SyncResult>) -> Self {
        Self {
            sync_arg: SyncArg::Parent(*digest),
            tx: Some(tx),
        }
    }

    fn new(
        digests: &ExecutionDigests,
        peer: AuthorityName,
        tx: oneshot::Sender<SyncResult>,
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

    /// Sync a cert which is appears as a parent in the verified effects of some other cert,
    /// and is thus known to be final.
    Parent(TransactionDigest),

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
            SyncArg::Parent(digest) | SyncArg::ExecDriver(digest) => (digest, None),
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
    state: Arc<AuthorityState>,
    pub(super) node_sync_store: Arc<NodeSyncStore>,
    aggregator: Arc<AuthorityAggregator<A>>,

    // Used to single-shot multiple concurrent downloads.
    pending_downloads: Arc<Waiter<TransactionDigest, SuiResult>>,

    // Used to wait for parent transactions to be applied locally
    pending_txes: Waiter<TransactionDigest, SyncResult>,

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

    pub fn store(&self) -> Arc<NodeSyncStore> {
        self.node_sync_store.clone()
    }
}

#[derive(Clone)]
pub enum SyncStatus {
    /// The digest has been successfully processed, but there is not yet sufficient stake
    /// voting for the digest to prove finality.
    NotFinal,

    /// The digest was executed locally.
    CertExecuted,
}

pub type SyncResult = SuiResult<SyncStatus>;

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
                let res = timeout(
                    MAX_NODE_TASK_LIFETIME,
                    state.process_digest(sync_arg, permit),
                )
                .await
                .map_err(|_| SuiError::TimeoutError);

                let digest = sync_arg.transaction_digest();

                let res = match res {
                    Err(error) | Ok(Err(error)) => {
                        error!(?digest, "process_digest failed: {}", error);
                        Err(error)
                    }

                    Ok(Ok(res)) => {
                        // Garbage collect data for this tx.
                        if let SyncStatus::CertExecuted = res {
                            state.cleanup_cert(digest);
                        }
                        Ok(res)
                    }
                };

                // Notify waiters even if tx failed, to avoid leaking resources.
                trace!(?digest, "notifying waiters");
                state
                    .pending_txes
                    .notify(digest, res.clone())
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

    async fn get_true_effects(
        &self,
        cert: &CertifiedTransaction,
    ) -> SuiResult<SignedTransactionEffects> {
        let digest = cert.digest();
        match self.node_sync_store.get_effects(digest)? {
            Some(effects) => Ok(effects),
            None => {
                let effects = self.aggregator.execute_cert_to_true_effects(cert).await?;
                self.node_sync_store.store_effects(digest, &effects)?;
                Ok(effects)
            }
        }
    }

    async fn get_cert(&self, digest: &TransactionDigest) -> SuiResult<CertifiedTransaction> {
        match self.state.database.read_certificate(digest)? {
            Some(cert) => Ok(cert),
            None => {
                let (cert, _) = self
                    .download_cert_and_effects(None, &DownloadRequest::Validator(*digest))
                    .await?;
                Ok(cert)
            }
        }
    }

    async fn process_parent_request(
        &self,
        permit: OwnedSemaphorePermit,
        digest: &TransactionDigest,
    ) -> SyncResult {
        trace!(?digest, "parent certificate execution requested");

        let cert = self.get_cert(digest).await?;
        let effects = self.get_true_effects(&cert).await?;
        match self
            .state
            .handle_node_sync_certificate(cert.clone(), effects.clone())
            .await
        {
            Ok(_) => Ok(SyncStatus::CertExecuted),
            Err(SuiError::ObjectErrors { .. }) => {
                debug!(?digest, "cert execution failed due to missing parents");

                // Must release permit before enqueuing new work to prevent deadlock.
                std::mem::drop(permit);

                self.enqueue_parent_execution_requests(&effects, false)
                    .await?;

                // Parents have been executed, so this should now succeed.
                debug!(?digest, "parents executed, re-attempting cert");
                self.state
                    .handle_node_sync_certificate(cert.clone(), effects.clone())
                    .await?;
                Ok(SyncStatus::CertExecuted)
            }
            Err(e) => Err(e),
        }
    }

    async fn process_exec_driver_digest(
        &self,
        permit: OwnedSemaphorePermit,
        digest: &TransactionDigest,
    ) -> SyncResult {
        trace!(?digest, "validator pending execution requested");
        let cert = self.get_cert(digest).await?;

        match self.state.handle_certificate(cert.clone()).await {
            Ok(_) => Ok(SyncStatus::CertExecuted),
            Err(SuiError::ObjectErrors { .. }) => {
                debug!(?digest, "cert execution failed due to missing parents");

                let effects = self.get_true_effects(&cert).await?;

                // Must release permit before enqueuing new work to prevent deadlock.
                std::mem::drop(permit);

                self.enqueue_parent_execution_requests(&effects, true)
                    .await?;

                // Parents have been executed, so this should now succeed.
                debug!(?digest, "parents executed, re-attempting cert");
                self.state.handle_certificate(cert.clone()).await?;
                Ok(SyncStatus::CertExecuted)
            }
            Err(e) => Err(e),
        }
    }

    pub fn cleanup_cert(&self, digest: &TransactionDigest) {
        debug!(?digest, "cleaning up temporary sync data");
        let _ = self
            .node_sync_store
            .cleanup_cert(digest)
            .tap_err(|e| warn!("cleanup_cert failed: {}", e));
    }

    async fn process_digest(&self, arg: SyncArg, permit: OwnedSemaphorePermit) -> SyncResult {
        trace!(?arg, "process_digest");

        let digest = arg.transaction_digest();

        // check if the tx is already locally final
        if self.state.database.effects_exists(digest)? {
            return Ok(SyncStatus::CertExecuted);
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
                return self.process_exec_driver_digest(permit, &digest).await;
            }
            SyncArg::Parent(digest) => {
                // digest is known to be final because it appeared in the dependencies list of a
                // verified TransactionEffects
                return self.process_parent_request(permit, &digest).await;
            }
            SyncArg::Follow(peer, digests) => {
                // Check if the tx is final.
                let stake = self.committee.weight(&peer);
                let quorum_threshold = self.committee.quorum_threshold();

                self.node_sync_store.record_effects_vote(
                    peer,
                    digests.transaction,
                    digests.effects,
                    stake,
                )?;
                let votes = self
                    .node_sync_store
                    .count_effects_votes(digests.transaction, digests.effects)?;

                let is_final = votes >= quorum_threshold;

                if !is_final {
                    return Ok(SyncStatus::NotFinal);
                }

                debug!(?digests, ?peer, "digests are now final");

                (
                    digests,
                    Some(
                        self.node_sync_store
                            .get_voters(digests.transaction, digests.effects)?,
                    ),
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

        let (is_first, mut rx) = self.pending_txes.wait(&digests.transaction);
        if !is_first {
            debug!(?digest, "tx is already in-progress, waiting...");
            return rx
                .recv()
                .await
                .map_err(|e| SuiError::GenericAuthorityError {
                    error: format!("{:?}", e),
                })?;
        }

        // Download the cert and effects - either finality has been establish (above), or
        // we are a validator.
        let (cert, effects) = self
            .download_cert_and_effects(authorities_with_cert, &DownloadRequest::Node(digests))
            .await?;

        let effects = effects
            // This error indicates a bug - download_cert_and_effects should never fail to return
            // effects when given a DownloadRequest::Node argument.
            .ok_or_else(|| SuiError::GenericAuthorityError {
                error: format!("effects for {:?} should have been fetched", digest),
            })
            .tap_err(|e| error!(?digest, "error: {}", e))?;

        self.process_parents(permit, &digests.transaction, &effects)
            .await?;

        self.state
            .handle_node_sync_certificate(cert, effects.clone())
            .await?;

        Ok(SyncStatus::CertExecuted)
    }

    async fn process_parents(
        &self,
        permit: OwnedSemaphorePermit,
        digest: &TransactionDigest,
        effects: &SignedTransactionEffects,
    ) -> SuiResult {
        // Node sync requests arrive in causal order via the follower API,
        // so in general the parents of a cert should have been enqueued already. However, it is
        // not guaranteed that the parents will reach finality (from our perspective) in a timely
        // manner - for instance if we are waiting for one more validator to send us the digest of
        // a parent certificate, there may be an unbounded number of other digests that precede
        // the parent.
        //
        // Therefore, we wait some period of time, and then take matters into our own hands.
        if let Ok(res) = timeout(
            PARENT_WAIT_TIMEOUT,
            self.wait_for_parents(permit, digest, effects),
        )
        .await
        {
            return res;
        }

        // The parents of a certificate are guaranteed to be final, so we can execute them
        // immediately via the exec driver path.
        debug!(
            ?digest,
            "wait_for_parents timed out, actively processing parents"
        );

        if let Err(err) = self
            .enqueue_parent_execution_requests(effects, false /* not validator */)
            .await
        {
            let msg = "enqueue_parent_execution_requests failed";
            debug!(?digest, parents = ?effects.effects.dependencies, "{}", msg);
            Err(err)
        } else {
            debug!(?digest, "All parent certificates executed");
            Ok(())
        }
    }

    async fn enqueue_parent_execution_requests(
        &self,
        effects: &SignedTransactionEffects,
        is_validator: bool,
    ) -> SuiResult {
        let parents = &effects.effects.dependencies;

        debug!(?parents, "attempting to execute parents");

        let handle = NodeSyncHandle::new_from_sender(self.sender.clone(), self.metrics.clone());
        let errors: Vec<_> = if is_validator {
            handle
                .handle_execution_request(parents.iter().cloned())
                .await?
                .filter_map(|r| r.err())
                .collect()
                .await
        } else {
            handle
                .handle_parents_request(parents.iter().cloned())
                .await?
                .filter_map(|r| r.err())
                .collect()
                .await
        };

        if errors.is_empty() {
            Ok(())
        } else {
            Err(SuiError::ExecutionDriverError {
                digest: effects.effects.transaction_digest,
                msg: "Could not execute all parent certificates".into(),
                errors,
            })
        }
    }

    async fn wait_for_parents(
        &self,
        permit: OwnedSemaphorePermit,
        digest: &TransactionDigest,
        effects: &SignedTransactionEffects,
    ) -> SuiResult {
        // Must drop the permit before waiting to avoid deadlock.
        std::mem::drop(permit);

        for parent in effects.effects.dependencies.iter() {
            let (_, mut rx) = self.pending_txes.wait(parent);

            if self.state.database.effects_exists(parent)? {
                continue;
            }

            trace!(?parent, ?digest, "waiting for parent");
            // Since we no longer hold the semaphore permit, can be sure that our parent will be
            // able to start.
            rx.recv()
                .await
                .map(|_| ())
                .map_err(|e| SuiError::GenericAuthorityError {
                    error: format!("{:?}", e),
                })?
        }

        if cfg!(debug_assertions) {
            for parent in effects.effects.dependencies.iter() {
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
    ) -> SuiResult<(CertifiedTransaction, Option<SignedTransactionEffects>)> {
        let tx_digest = *req.transaction_digest();

        match (req, self.node_sync_store.get_cert_and_effects(&tx_digest)?) {
            (DownloadRequest::Node(_), (Some(cert), Some(effects))) => {
                return Ok((cert, Some(effects)))
            }
            (DownloadRequest::Validator(_), (Some(cert), effects)) => return Ok((cert, effects)),
            _ => (),
        }

        let pending_downloads = self.pending_downloads.clone();
        let (first, mut rx) = pending_downloads.wait(&tx_digest);
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

        let cert = self.node_sync_store.get_cert(&tx_digest)?.ok_or_else(|| {
            SuiError::GenericAuthorityError {
                error: format!(
                    "cert/effects for {:?} should have been in the node_sync_store",
                    tx_digest
                ),
            }
        })?;

        let effects = self.node_sync_store.get_effects(&tx_digest)?;

        Ok((cert, effects))
    }

    async fn download_impl(
        authorities: Option<BTreeSet<AuthorityName>>,
        aggregator: Arc<AuthorityAggregator<A>>,
        req: &DownloadRequest,
        node_sync_store: Arc<NodeSyncStore>,
        metrics: GossipMetrics,
    ) -> SuiResult {
        match req {
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
                node_sync_store.store_cert(&resp.0)?;
                node_sync_store.store_effects(req.transaction_digest(), &resp.1)?;
            }
            DownloadRequest::Validator(digest) => {
                let resp = aggregator.handle_cert_info_request(digest, None).await?;
                match resp {
                    TransactionInfoResponse {
                        certified_transaction: Some(cert),
                        ..
                    } => {
                        // can only store cert here, effects are not verified yet.
                        node_sync_store.store_cert(&cert)?;
                    }
                    _ => return Err(SuiError::TransactionNotFound { digest: *digest }),
                }
            }
        };

        Ok(())
    }
}

/// A cloneable handle that can send messages to a NodeSyncState
#[derive(Clone)]
pub struct NodeSyncHandle {
    sender: mpsc::Sender<DigestsMessage>,
    pub metrics: GossipMetrics,
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

    fn map_rx(rx: oneshot::Receiver<SyncResult>) -> BoxFuture<'static, SyncResult> {
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

    /// Sync transactions in certified checkpoint. Since the checkpoint is certified,
    /// we can fully trust the effect digests in the checkpoint content.
    pub async fn sync_checkpoint_cert_transactions(
        &self,
        checkpoint_contents: &CheckpointContents,
    ) -> SuiResult<impl Stream<Item = SyncResult>> {
        let mut futures = FuturesOrdered::new();
        for digests in checkpoint_contents.iter() {
            let (tx, rx) = oneshot::channel();
            let msg = DigestsMessage::new_for_ckpt(digests, tx);
            Self::send_msg_with_tx(self.sender.clone(), msg).await?;
            futures.push_back(Self::map_rx(rx));
        }

        Ok(futures)
    }

    /// Sync a to-be-signed checkpoint transactions. Since we don't have a cert
    /// yet, the effects digests cannot be trusted. We rely on the transaction
    /// digest only for the sync.
    /// TODO: This shall eventually be able to bypass the validator halt.
    pub async fn sync_pending_checkpoint_transactions(
        &self,
        transactions: impl Iterator<Item = &ExecutionDigests>,
    ) -> SuiResult<impl Stream<Item = SyncResult>> {
        let mut futures = FuturesOrdered::new();
        for digests in transactions {
            let (tx, rx) = oneshot::channel();
            let msg = DigestsMessage::new_for_exec_driver(&digests.transaction, tx);
            Self::send_msg_with_tx(self.sender.clone(), msg).await?;
            futures.push_back(Self::map_rx(rx));
        }

        Ok(futures)
    }

    pub async fn handle_execution_request(
        &self,
        digests: impl Iterator<Item = TransactionDigest>,
    ) -> SuiResult<impl Stream<Item = SyncResult>> {
        let mut futures = FuturesOrdered::new();
        for digest in digests {
            let (tx, rx) = oneshot::channel();
            let msg = DigestsMessage::new_for_exec_driver(&digest, tx);
            Self::send_msg_with_tx(self.sender.clone(), msg).await?;
            futures.push_back(Self::map_rx(rx));
        }

        Ok(futures)
    }

    pub async fn handle_parents_request(
        &self,
        digests: impl Iterator<Item = TransactionDigest>,
    ) -> SuiResult<impl Stream<Item = SyncResult>> {
        let mut futures = FuturesOrdered::new();
        for digest in digests {
            let (tx, rx) = oneshot::channel();
            let msg = DigestsMessage::new_for_parents(&digest, tx);
            Self::send_msg_with_tx(self.sender.clone(), msg).await?;
            futures.push_back(Self::map_rx(rx));
        }

        Ok(futures)
    }

    pub async fn handle_sync_digest(
        &self,
        peer: AuthorityName,
        digests: ExecutionDigests,
    ) -> SuiResult<BoxFuture<'static, SyncResult>> {
        let (tx, rx) = oneshot::channel();
        let sender = self.sender.clone();
        Self::send_msg_with_tx(sender, DigestsMessage::new(&digests, peer, tx)).await?;
        Ok(Self::map_rx(rx))
    }
}
