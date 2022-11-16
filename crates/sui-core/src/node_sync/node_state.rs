// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::AuthorityState, authority_active::gossip::GossipMetrics,
    authority_active::ActiveAuthority, authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
};

use tokio_stream::{Stream, StreamExt};

use std::collections::{hash_map, BTreeSet, HashMap};
use sui_metrics::monitored_future;
use sui_metrics::spawn_monitored_task;
use sui_storage::node_sync_store::NodeSyncStore;
use sui_types::{
    base_types::{
        AuthorityName, EpochId, ExecutionDigests, TransactionDigest, TransactionEffectsDigest,
    },
    error::{SuiError, SuiResult},
    messages::{
        CertifiedTransaction, SignedTransactionEffects, TransactionEffects,
        TransactionInfoResponse, VerifiedCertificate,
    },
    messages_checkpoint::CheckpointContents,
};

use std::ops::Deref;
use std::sync::{Arc, Mutex};

use futures::{future::BoxFuture, stream::FuturesOrdered, FutureExt};

use tap::TapFallible;

use tokio::sync::{broadcast, mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};

use tracing::{debug, error, instrument, trace, warn};

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

macro_rules! check_epoch {
    ($expected_epoch: expr, $observed_epoch: expr) => {
        let expected_epoch = $expected_epoch;
        let observed_epoch = $observed_epoch;

        // Debug assert ok - execution will not continue with broken invariant in release mode due
        // to error return below.
        debug_assert_eq!(expected_epoch, observed_epoch);

        if expected_epoch != observed_epoch {
            // Most likely indicates a reconfiguration bug.
            error!(?expected_epoch, ?observed_epoch, "Epoch mismatch");
            return Err(SuiError::WrongEpoch {
                expected_epoch,
                actual_epoch: observed_epoch,
            });
        }
    };
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

    fn notify(&self, key: &Key, res: ResultT) -> SuiResult {
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
    epoch_id: EpochId,
    tx: Option<oneshot::Sender<SyncResult>>,
}

impl DigestsMessage {
    fn new_for_ckpt(
        epoch_id: EpochId,
        digests: &ExecutionDigests,
        tx: oneshot::Sender<SyncResult>,
    ) -> Self {
        Self {
            epoch_id,
            sync_arg: SyncArg::Checkpoint(*digests),
            tx: Some(tx),
        }
    }

    fn new_for_pending_ckpt(
        epoch_id: EpochId,
        digest: &TransactionDigest,
        tx: oneshot::Sender<SyncResult>,
    ) -> Self {
        Self {
            epoch_id,
            sync_arg: SyncArg::PendingCheckpoint(*digest),
            tx: Some(tx),
        }
    }

    fn new_for_exec_driver(
        epoch_id: EpochId,
        digest: &TransactionDigest,
        tx: oneshot::Sender<SyncResult>,
    ) -> Self {
        Self {
            epoch_id,
            sync_arg: SyncArg::ExecDriver(*digest),
            tx: Some(tx),
        }
    }

    fn new_for_parents(
        epoch_id: EpochId,
        digest: &TransactionDigest,
        tx: oneshot::Sender<SyncResult>,
    ) -> Self {
        Self {
            epoch_id,
            sync_arg: SyncArg::Parent(*digest),
            tx: Some(tx),
        }
    }

    fn new(
        epoch_id: EpochId,
        digests: &ExecutionDigests,
        peer: AuthorityName,
        tx: oneshot::Sender<SyncResult>,
    ) -> Self {
        Self {
            epoch_id,
            sync_arg: SyncArg::Follow(peer, *digests),
            tx: Some(tx),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum SyncArg {
    /// In follow mode, wait for 2f+1 votes for a tx before executing
    Follow(AuthorityName, ExecutionDigests),

    /// Sync a cert that is finalized. It may appear as a parent in the verified effects of some
    /// other cert, or come from the Transaction Orchestrator.
    Parent(TransactionDigest),

    /// In checkpoint mode, all txes are known to be final.
    Checkpoint(ExecutionDigests),

    /// Transactions in the current checkpoint to be signed/stored.
    /// We don't have the effect digest since we may not have it when constructing the checkpoint.
    /// The primary difference between PendingCheckpoint and ExecDriver is that PendingCheckpoint
    /// sync can by-pass validator halting. This is to ensure that the last checkpoint of the epoch
    /// can always be formed when there are missing transactions.
    PendingCheckpoint(TransactionDigest),

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
            SyncArg::Parent(digest)
            | SyncArg::ExecDriver(digest)
            | SyncArg::PendingCheckpoint(digest) => (digest, None),
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
    active_authority: Arc<ActiveAuthority<A>>,

    // Used to single-shot multiple concurrent downloads.
    pending_downloads: Arc<Waiter<TransactionDigest, SuiResult>>,

    // Used to wait for parent transactions to be applied locally
    pending_parents: Waiter<TransactionDigest, SyncResult>,

    // Used to suppress duplicate tx processing.
    pending_txes: Waiter<TransactionDigest, SyncResult>,

    // Channels for enqueuing DigestMessage requests.
    sender: mpsc::Sender<DigestsMessage>,
    receiver: Arc<tokio::sync::Mutex<mpsc::Receiver<DigestsMessage>>>,
}

impl<A> NodeSyncState<A> {
    pub fn new(active_authority: Arc<ActiveAuthority<A>>) -> Self {
        let (sender, receiver) = mpsc::channel(NODE_SYNC_QUEUE_LEN);
        Self {
            active_authority,
            pending_downloads: Arc::new(Waiter::new()),
            pending_parents: Waiter::new(),
            pending_txes: Waiter::new(),
            sender,
            receiver: Arc::new(tokio::sync::Mutex::new(receiver)),
        }
    }

    pub fn store(&self) -> Arc<NodeSyncStore> {
        self.state().node_sync_store.clone()
    }

    fn state(&self) -> &AuthorityState {
        &self.active_authority.state
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
                    spawn_monitored_task!(monitored_future!(async move {
                        let _guard = state.receiver.lock().await;
                    })),
                    sender,
                );
            }
            Ok(r) => r,
        };

        (
            spawn_monitored_task!(monitored_future!(state.handle_messages(&mut receiver))),
            sender,
        )
    }

    fn notify(
        waiter: &Waiter<TransactionDigest, SyncResult>,
        digest: &TransactionDigest,
        result: SyncResult,
    ) {
        waiter
            .notify(digest, result)
            .tap_err(|e| debug!(?digest, "{}", e))
            .ok();
    }

    #[instrument(name = "record_vote_and_check_finality", level = "trace", skip_all)]
    fn record_vote_and_check_finality(
        &self,
        peer: &AuthorityName,
        digests: &ExecutionDigests,
        epoch_id: EpochId,
    ) -> SuiResult<bool> {
        // Check if the tx is final.
        let committee = self.state().committee.load();
        check_epoch!(committee.epoch, epoch_id);
        let stake = committee.weight(peer);
        let quorum_threshold = committee.quorum_threshold();

        self.store().record_effects_vote(
            epoch_id,
            *peer,
            digests.transaction,
            digests.effects,
            stake,
        )?;
        let votes =
            self.store()
                .count_effects_votes(epoch_id, digests.transaction, digests.effects)?;

        Ok(votes >= quorum_threshold)
    }

    async fn handle_messages(self: Arc<Self>, receiver: &mut mpsc::Receiver<DigestsMessage>) {
        // this pattern for limiting concurrency is from
        // https://github.com/tokio-rs/tokio/discussions/2648
        let limit = Arc::new(Semaphore::new(MAX_NODE_SYNC_CONCURRENCY));

        while let Some(DigestsMessage {
            epoch_id,
            sync_arg,
            tx,
        }) = receiver.recv().await
        {
            let state = self.clone();

            // For Follow message: we record vote and check finality here so
            // it's not rate limited. A semaphore is only needed when the
            // digest is final and ready to be executed.
            if let SyncArg::Follow(peer, exec_digest) = sync_arg {
                match self.record_vote_and_check_finality(&peer, &exec_digest, epoch_id) {
                    Ok(false) => {
                        trace!(tx_digest=?exec_digest.transaction, effects_digest=?exec_digest.effects, ?peer, "digests is not final");
                        Self::send_sync_res_to_receiver(
                            tx,
                            Ok(SyncStatus::NotFinal),
                            &sync_arg,
                            &epoch_id,
                        );
                        continue; // tx is not final, do nothing
                    }
                    Ok(true) => {
                        debug!(tx_digest=?exec_digest.transaction, effects_digest=?exec_digest.effects, ?peer, "digests are now final")
                    }
                    Err(err) => {
                        error!(tx_digest=?exec_digest.transaction, effects_digest=?exec_digest.effects, ?peer, "failed to record vote and check finality: {}", err);
                        Self::send_sync_res_to_receiver(tx, Err(err), &sync_arg, &epoch_id);
                        // error when checking finality, skip and wait for the next digest or checkpoint to re-trigger
                        continue;
                    }
                }
            }

            let limit = limit.clone();

            // hold semaphore permit until task completes. unwrap ok because we never close
            // the semaphore in this context.
            let permit = limit.acquire_owned().await.unwrap();

            spawn_monitored_task!(async move {
                let res = timeout(
                    MAX_NODE_TASK_LIFETIME,
                    state.process_digest(epoch_id, sync_arg, permit),
                )
                .await
                .map_err(|_| SuiError::TimeoutError);

                let tx_digest = sync_arg.transaction_digest();

                let res = match res {
                    Err(error) | Ok(Err(error)) => {
                        if matches!(error, SuiError::ValidatorHaltedAtEpochEnd) {
                            // This is not a real error.
                            debug!(?tx_digest, "process_digest failed: {}", error);
                        } else {
                            error!(?tx_digest, "process_digest failed: {}", error);
                        }
                        Err(error)
                    }

                    Ok(Ok(res)) => {
                        // Garbage collect data for this tx.
                        if let SyncStatus::CertExecuted = res {
                            state.cleanup_cert(epoch_id, tx_digest);
                        }
                        Ok(res)
                    }
                };

                // Notify waiters even if tx failed, to avoid leaking resources.
                trace!(?epoch_id, ?tx_digest, "notifying parents and waiters");
                Self::notify(&state.pending_parents, tx_digest, res.clone());
                Self::notify(&state.pending_txes, tx_digest, res.clone());

                Self::send_sync_res_to_receiver(tx, res, &sync_arg, &epoch_id);
            });
        }
    }

    fn send_sync_res_to_receiver(
        tx: Option<oneshot::Sender<Result<SyncStatus, SuiError>>>,
        res: Result<SyncStatus, SuiError>,
        sync_arg: &SyncArg,
        epoch_id: &EpochId,
    ) {
        if let Some(tx) = tx {
            if tx.send(res).is_err() {
                // This will happen any time the follower times out and restarts, but
                // that's ok - the follower won't have marked this digest as processed so it
                // will be retried.
                debug!(
                    ?sync_arg,
                    ?epoch_id,
                    "could not send process_digest response to caller",
                );
            }
        }
    }

    fn aggregator(&self) -> Arc<AuthorityAggregator<A>> {
        self.active_authority.net.load().deref().clone()
    }

    async fn get_true_effects(
        &self,
        epoch_id: EpochId,
        cert: &CertifiedTransaction,
    ) -> SuiResult<SignedTransactionEffects> {
        let digest = cert.digest();

        check_epoch!(epoch_id, cert.epoch());

        match self.store().get_effects(epoch_id, digest)? {
            Some(effects) => Ok(effects),
            None => {
                let aggregator = self.aggregator();
                let effects = aggregator.execute_cert_to_true_effects(cert).await?;
                self.store().store_effects(epoch_id, digest, &effects)?;
                Ok(effects)
            }
        }
    }

    async fn get_cert(
        &self,
        epoch_id: EpochId,
        digest: &TransactionDigest,
    ) -> SuiResult<VerifiedCertificate> {
        if let Some(cert) = self.store().get_cert(epoch_id, digest)? {
            assert_eq!(epoch_id, cert.epoch());
            return Ok(cert);
        }

        let (cert, _) = self
            .download_cert_and_effects(epoch_id, None, &DownloadRequest::Validator(*digest))
            .await?;
        Ok(cert)
    }

    fn get_missing_parents(
        &self,
        effects: &TransactionEffects,
    ) -> SuiResult<Vec<TransactionDigest>> {
        let mut missing_parents = Vec::new();
        for parent in effects.dependencies.iter() {
            if !self.state().database.effects_exists(parent)? {
                missing_parents.push(*parent);
            }
        }
        Ok(missing_parents)
    }

    async fn process_parent_request(
        &self,
        permit: OwnedSemaphorePermit,
        epoch_id: EpochId,
        digest: &TransactionDigest,
    ) -> SyncResult {
        trace!(?digest, "parent certificate execution requested");

        // Note that parents can be from previous epochs. However, when we attempt are trying to
        // sync parents of certs in epoch N, if the parent is from epoch P < N, the parent must
        // already be final, so we shouldn't get this far. Since the parent therefore must be from
        // the same epoch, we can assume the same epoch_id will hold.
        let cert = self.get_cert(epoch_id, digest).await?;
        let effects = self.get_true_effects(epoch_id, &cert).await?;

        // Must release permit before enqueuing new work to prevent deadlock.
        drop(permit);

        let missing_parents = self.get_missing_parents(effects.data())?;
        self.enqueue_parent_execution_requests(epoch_id, digest, &missing_parents, false)
            .await?;

        match self
            .state()
            .handle_certificate_with_effects(&cert, &effects)
            .await
        {
            Ok(_) => Ok(SyncStatus::CertExecuted),
            Err(e) => Err(e),
        }
    }

    async fn process_exec_driver_digest(
        &self,
        epoch_id: EpochId,
        permit: OwnedSemaphorePermit,
        digest: &TransactionDigest,
        // TODO: This call path will all be deleted.
        _bypass_validator_halt: bool,
    ) -> SyncResult {
        trace!(?digest, "validator pending execution requested");

        let cert = self.get_cert(epoch_id, digest).await?;

        let result = self.state().handle_certificate(&cert).await;
        match result {
            Ok(_) => Ok(SyncStatus::CertExecuted),
            e @ Err(SuiError::TransactionInputObjectsErrors { .. }) => {
                debug!(
                    ?digest,
                    "cert execution failed due to missing parents {:?}", e
                );

                let effects = self.get_true_effects(epoch_id, &cert).await?;

                // Must release permit before enqueuing new work to prevent deadlock.
                drop(permit);

                let missing_parents = self.get_missing_parents(effects.data())?;
                self.enqueue_parent_execution_requests(epoch_id, digest, &missing_parents, true)
                    .await?;

                // Parents have been executed, so this should now succeed.
                debug!(?digest, "parents executed, re-attempting cert");
                self.state().handle_certificate(&cert).await?;
                Ok(SyncStatus::CertExecuted)
            }
            Err(e) => Err(e),
        }
    }

    pub fn cleanup_cert(&self, epoch_id: EpochId, digest: &TransactionDigest) {
        debug!(?digest, "cleaning up temporary sync data");
        let _ = self
            .store()
            .cleanup_cert(epoch_id, digest)
            .tap_err(|e| warn!("cleanup_cert failed: {}", e));
    }

    async fn process_digest(
        &self,
        epoch_id: EpochId,
        arg: SyncArg,
        permit: OwnedSemaphorePermit,
    ) -> SyncResult {
        trace!(?arg, "process_digest");

        let digest = arg.transaction_digest();

        // check if the tx is already locally final
        if self.state().database.effects_exists(digest)? {
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
                return self
                    .process_exec_driver_digest(epoch_id, permit, &digest, false)
                    .await;
            }
            SyncArg::PendingCheckpoint(digest) => {
                return self
                    .process_exec_driver_digest(epoch_id, permit, &digest, true)
                    .await;
            }
            SyncArg::Parent(digest) => {
                // digest is known to be final because it either appeared in
                // the dependencies list of a verified TransactionEffects, or
                // is passed from TransactionOrchestrator
                return self.process_parent_request(permit, epoch_id, &digest).await;
            }
            SyncArg::Follow(_peer, digests) => {
                // We checked the finality of digests earlier and know that it is final.
                (
                    digests,
                    Some(self.store().get_voters(
                        epoch_id,
                        digests.transaction,
                        digests.effects,
                    )?),
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
            .download_cert_and_effects(
                epoch_id,
                authorities_with_cert,
                &DownloadRequest::Node(digests),
            )
            .await?;

        let effects = effects
            // This error indicates a bug - download_cert_and_effects should never fail to return
            // effects when given a DownloadRequest::Node argument.
            .ok_or_else(|| SuiError::GenericAuthorityError {
                error: format!("effects for {:?} should have been fetched", digest),
            })
            .tap_err(|e| error!(?digest, "error: {}", e))?;

        self.process_parents(permit, epoch_id, &digests.transaction, &effects)
            .await?;

        self.state()
            .handle_certificate_with_effects(&cert, &effects)
            .await?;

        Ok(SyncStatus::CertExecuted)
    }

    async fn process_parents(
        &self,
        permit: OwnedSemaphorePermit,
        epoch_id: EpochId,
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

        let missing_parents = self.get_missing_parents(effects.data())?;

        if let Err(err) = self
            .enqueue_parent_execution_requests(
                epoch_id,
                digest,
                &missing_parents,
                false, /* not validator */
            )
            .await
        {
            let msg = "enqueue_parent_execution_requests failed";
            debug!(?digest, parents = ?effects.data().dependencies, "{}", msg);
            Err(err)
        } else {
            debug!(?digest, "All parent certificates executed");
            Ok(())
        }
    }

    async fn enqueue_parent_execution_requests(
        &self,
        epoch_id: EpochId,
        digest: &TransactionDigest,
        parents: &[TransactionDigest],
        is_validator: bool,
    ) -> SuiResult {
        debug!(?parents, "attempting to execute parents");

        let handle = NodeSyncHandle::new_from_sender(
            self.sender.clone(),
            self.active_authority.gossip_metrics.clone(),
        );
        let errors: Vec<_> = if is_validator {
            handle
                .handle_execution_request(epoch_id, parents.iter().cloned())
                .await?
                .filter_map(|r| r.err())
                .collect()
                .await
        } else {
            handle
                .handle_parents_request(epoch_id, parents.iter().cloned())
                .await?
                .filter_map(|r| r.err())
                .collect()
                .await
        };

        if errors.is_empty() {
            Ok(())
        } else {
            Err(SuiError::ExecutionDriverError {
                digest: *digest,
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
        drop(permit);

        for parent in effects.data().dependencies.iter() {
            let (_, mut rx) = self.pending_parents.wait(parent);

            if self.state().database.effects_exists(parent)? {
                continue;
            }

            trace!(?parent, ?digest, "waiting for parent");
            // Since we no longer hold the semaphore permit, can be sure that our parent will be
            // able to start.
            rx.recv()
                .await
                .map_err(|e| SuiError::GenericAuthorityError {
                    error: format!("{:?}", e),
                })??;
        }

        if cfg!(debug_assertions) {
            for parent in effects.data().dependencies.iter() {
                debug_assert!(self.state().database.effects_exists(parent).unwrap());
            }
        }

        Ok(())
    }

    // Download the certificate and effects specified in digests.
    // TODO: In checkpoint mode, we don't need to download a cert, a transaction will do.
    // Transactions are not currently persisted anywhere, however (validators delete them eagerly).
    async fn download_cert_and_effects(
        &self,
        epoch_id: EpochId,
        authorities_with_cert: Option<BTreeSet<AuthorityName>>,
        req: &DownloadRequest,
    ) -> SuiResult<(VerifiedCertificate, Option<SignedTransactionEffects>)> {
        let tx_digest = *req.transaction_digest();

        match (
            req,
            self.store().get_cert_and_effects(epoch_id, &tx_digest)?,
        ) {
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
            let aggregator = self.aggregator();
            let node_sync_store = self.store();
            let req = req.clone();
            let metrics = self.active_authority.gossip_metrics.clone();
            spawn_monitored_task!(async move {
                let _ = pending_downloads.notify(
                    &tx_digest,
                    Self::download_impl(
                        epoch_id,
                        authorities_with_cert,
                        aggregator,
                        &req,
                        node_sync_store,
                        metrics,
                    )
                    .await,
                );
            });
        }

        rx.recv()
            .await
            .map_err(|e| SuiError::GenericAuthorityError {
                error: format!("{:?}", e),
            })??;

        let cert = self
            .store()
            .get_cert(epoch_id, &tx_digest)?
            .ok_or_else(|| SuiError::GenericAuthorityError {
                error: format!(
                    "cert/effects for {:?} should have been in the node_sync_store",
                    tx_digest
                ),
            })?;

        let effects = self.store().get_effects(epoch_id, &tx_digest)?;

        Ok((cert, effects))
    }

    async fn download_impl(
        epoch_id: EpochId,
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
                node_sync_store.store_cert(epoch_id, &resp.0)?;
                node_sync_store.store_effects(epoch_id, req.transaction_digest(), &resp.1)?;
            }
            DownloadRequest::Validator(digest) => {
                let resp = aggregator.handle_cert_info_request(digest, None).await?;
                match resp {
                    TransactionInfoResponse {
                        certified_transaction: Some(cert),
                        ..
                    } => {
                        // can only store cert here, effects are not verified yet.
                        node_sync_store.store_cert(epoch_id, &cert)?;
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
        epoch_id: EpochId,
        checkpoint_contents: &CheckpointContents,
    ) -> SuiResult<impl Stream<Item = SyncResult>> {
        let mut futures = FuturesOrdered::new();
        for digests in checkpoint_contents.iter() {
            let (tx, rx) = oneshot::channel();
            let msg = DigestsMessage::new_for_ckpt(epoch_id, digests, tx);
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
        epoch_id: EpochId,
        transactions: impl Iterator<Item = &ExecutionDigests>,
    ) -> SuiResult<impl Stream<Item = SyncResult>> {
        let mut futures = FuturesOrdered::new();
        for digests in transactions {
            let (tx, rx) = oneshot::channel();
            let msg = DigestsMessage::new_for_pending_ckpt(epoch_id, &digests.transaction, tx);
            Self::send_msg_with_tx(self.sender.clone(), msg).await?;
            futures.push_back(Self::map_rx(rx));
        }

        Ok(futures)
    }

    pub async fn handle_execution_request(
        &self,
        epoch_id: EpochId,
        digests: impl Iterator<Item = TransactionDigest>,
    ) -> SuiResult<impl Stream<Item = SyncResult>> {
        let mut futures = FuturesOrdered::new();
        for digest in digests {
            let (tx, rx) = oneshot::channel();
            let msg = DigestsMessage::new_for_exec_driver(epoch_id, &digest, tx);
            Self::send_msg_with_tx(self.sender.clone(), msg).await?;
            futures.push_back(Self::map_rx(rx));
        }

        Ok(futures)
    }

    pub async fn handle_parents_request(
        &self,
        epoch_id: EpochId,
        digests: impl Iterator<Item = TransactionDigest>,
    ) -> SuiResult<impl Stream<Item = SyncResult>> {
        let mut futures = FuturesOrdered::new();
        for digest in digests {
            let (tx, rx) = oneshot::channel();
            let msg = DigestsMessage::new_for_parents(epoch_id, &digest, tx);
            Self::send_msg_with_tx(self.sender.clone(), msg).await?;
            futures.push_back(Self::map_rx(rx));
        }

        Ok(futures)
    }

    pub async fn handle_sync_digest(
        &self,
        epoch_id: EpochId,
        peer: AuthorityName,
        digests: ExecutionDigests,
    ) -> SuiResult<BoxFuture<'static, SyncResult>> {
        let (tx, rx) = oneshot::channel();
        let sender = self.sender.clone();
        Self::send_msg_with_tx(sender, DigestsMessage::new(epoch_id, &digests, peer, tx)).await?;
        Ok(Self::map_rx(rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_execution_driver_error() {
        let digest = TransactionDigest::new([11u8; 32]);
        let err0 = SuiError::ExecutionDriverError {
            digest,
            msg: "test 0".into(),
            errors: Vec::new(),
        };
        let err1 = SuiError::ExecutionDriverError {
            digest,
            msg: "test 1".into(),
            errors: vec![err0],
        };
        let err2 = SuiError::ExecutionDriverError {
            digest,
            msg: "test 2".into(),
            errors: vec![err1],
        };
        assert_eq!(format!("{}", err2), "ExecutionDriver error for CwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCws=: test 2 - Caused by : [ ExecutionDriver error for CwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCws=: test 1 - Caused by : [ ExecutionDriver error for CwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCwsLCws=: test 0 - Caused by : [  ] ] ]");
    }
}
