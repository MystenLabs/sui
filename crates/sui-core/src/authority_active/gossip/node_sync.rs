// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{DigestHandler, Follower};
use crate::{
    authority::AuthorityState, authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI,
};
use async_trait::async_trait;

use std::collections::{hash_map, HashMap};
use sui_storage::node_sync_store::NodeSyncStore;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, TransactionDigest, TransactionEffectsDigest},
    committee::{Committee, StakeUnit},
    error::{SuiError, SuiResult},
    messages::{CertifiedTransaction, SignedTransactionEffects},
};
use sui_types::crypto::KeyPair;

use std::ops::Deref;
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;

use tracing::{debug, error, info, trace, warn};

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

    /// Returns (Some(tx), rx) if there are no other waiters yet, or else (None, rx).
    /// All rxes can be woken by sending to the supplied tx, or by calling notify(key, result)
    async fn wait(
        &self,
        key: &Key,
    ) -> (
        Option<broadcast::Sender<ResultT>>,
        broadcast::Receiver<ResultT>,
    ) {
        let waiters = &mut self.waiters.lock().unwrap();
        let entry = waiters.entry(key.clone());

        match entry {
            hash_map::Entry::Occupied(e) => (None, e.get().subscribe()),
            hash_map::Entry::Vacant(e) => {
                let (tx, rx) = broadcast::channel(1);
                e.insert(tx.clone());
                (Some(tx), rx)
            }
        }
    }

    async fn notify(&self, key: &Key, res: ResultT) -> SuiResult {
        if let Some(tx) = self.waiters.lock().unwrap().remove(key) {
            tx.send(res).map_err(|_| SuiError::GenericAuthorityError {
                error: format!("couldn't notify waiters for key {:?}", key),
            })?;
        }
        // else: no one was waiting on this key.
        Ok(())
    }
}

struct DigestsMessage {
    digests: ExecutionDigests,
    peer: AuthorityName,
    tx: oneshot::Sender<SuiResult>,
}

/// NodeSyncState is shared by any number of NodeSyncDigestHandler's, and receives DigestsMessage
/// messages from those handlers, waits for finality of TXes, and then downloads and applies those
/// TXes locally.
struct NodeSyncState<A> {
    committee: Arc<Committee>,
    effects_stake: Mutex<EffectsStakeMap>,
    state: Arc<AuthorityState>,
    node_sync_store: Arc<NodeSyncStore>,
    aggregator: Arc<AuthorityAggregator<A>>,

    // Used to single-shot multiple concurrent downloads.
    pending_downloads: Waiter<TransactionDigest, SuiResult>,

    // Used to wait for parent transactions to be applied locally
    pending_txes: Waiter<TransactionDigest, ()>,
}

impl<A> NodeSyncState<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    fn start(self, mut receiver: mpsc::Receiver<DigestsMessage>) -> JoinHandle<()> {
        let state = Arc::new(self);
        tokio::spawn(async move {
            // this pattern for limiting concurrency is from
            // https://github.com/tokio-rs/tokio/discussions/2648
            let limit = Arc::new(Semaphore::new(MAX_NODE_SYNC_CONCURRENCY));

            while let Some(DigestsMessage { digests, peer, tx }) = receiver.recv().await {
                let state = state.clone();
                let limit = limit.clone();
                tokio::spawn(async move {
                    // hold semaphore permit until task completes. unwrap ok because we never close
                    // the semaphore in this context.
                    let permit = limit.acquire_owned().await.unwrap();

                    let res = state.process_digest(peer, digests, permit).await;
                    if let Err(error) = &res {
                        error!(?digests, ?peer, "process_digest failed: {}", error);
                    }

                    // Send status back to follower so that it knows whether to advance
                    // the watermark.
                    if tx.send(res).is_err() {
                        // This will happen any time the follower times out and restarts, but
                        // that's ok - the follower won't have marked this digest as processed so it
                        // will be retried.
                        debug!(
                            ?digests,
                            ?peer,
                            "could not send process_digest response to caller",
                        );
                    }
                });
            }
        })
    }

    async fn process_digest(
        &self,
        peer: AuthorityName,
        digests: ExecutionDigests,
        permit: OwnedSemaphorePermit,
    ) -> SuiResult {
        trace!(?digests, ?peer, "process_digest");

        // check if we the tx is already locally final
        if self.state.database.effects_exists(&digests.transaction)? {
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
            let (_, mut rx) = self.pending_txes.wait(&digests.transaction).await;
            return rx
                .recv()
                .await
                .map_err(|e| SuiError::GenericAuthorityError {
                    error: format!("{:?}", e),
                });
        }

        trace!(?digests, ?peer, "digests are now final");

        // Download the cert and effects now that we have established finality and we know that the
        // effects digest is correct.
        let (cert, effects) = self.download_cert_and_effects(&peer, &digests).await?;

        // we're done downloading at this point, so we no longer need to prevent other tasks from
        // starting.
        std::mem::drop(permit);

        for parent in effects.effects.dependencies.iter() {
            let (_, mut rx) = self.pending_txes.wait(parent).await;

            if self.state.database.effects_exists(parent)? {
                continue;
            }

            trace!(?parent, digest = ?digests.transaction, "waiting for parent");
            // Since we no longer hold the semaphore permit, can be sure that our parent will be
            // able to start.
            rx.recv()
                .await
                .map_err(|e| SuiError::GenericAuthorityError {
                    error: format!("{:?}", e),
                })?;
        }

        if cfg!(debug_assertions) {
            for parent in effects.effects.dependencies.iter() {
                debug_assert!(self.state.database.effects_exists(parent).unwrap());
            }
        }

        self.state
            .handle_node_sync_transaction(cert, effects)
            .await?;

        // Garbage collect data for this tx.
        self.node_sync_store
            .delete_cert_and_effects(&digests.transaction)?;
        self.effects_stake
            .lock()
            .unwrap()
            .forget_effects(&digests.effects);

        // Notify waiting child transactions.
        trace!(digest = ?digests.transaction, "notifying parent");
        self.pending_txes.notify(&digests.transaction, ()).await?;
        Ok(())
    }

    // Download the certificate and effects specified in digests.
    async fn download_cert_and_effects(
        &self,
        peer: &AuthorityName,
        digests: &ExecutionDigests,
    ) -> SuiResult<(CertifiedTransaction, SignedTransactionEffects)> {
        let digest = digests.transaction;
        if let Some(c) = self.node_sync_store.get_cert_and_effects(&digest)? {
            return Ok(c);
        }

        let (tx, mut rx) = self.pending_downloads.wait(&digest).await;
        // Only start the download if there are no other concurrent downloads.
        if let Some(tx) = tx {
            let aggregator = self.aggregator.clone();
            let digests = *digests;
            let peer = *peer;
            let node_sync_store = self.node_sync_store.clone();
            tokio::task::spawn(async move {
                if let Err(error) =
                    tx.send(Self::download_impl(peer, aggregator, &digests, node_sync_store).await)
                {
                    error!(?digest, ?peer, ?error, "Could not broadcast cert response");
                }
            });
        }

        rx.recv()
            .await
            .map_err(|e| SuiError::GenericAuthorityError {
                error: format!("{:?}", e),
            })??;

        self.node_sync_store
            .get_cert_and_effects(&digest)?
            .ok_or_else(|| SuiError::GenericAuthorityError {
                error: format!(
                    "cert/effects for {:?} should have been in the node_sync_store",
                    digest
                ),
            })
    }

    async fn download_impl(
        peer: AuthorityName,
        aggregator: Arc<AuthorityAggregator<A>>,
        digests: &ExecutionDigests,
        node_sync_store: Arc<NodeSyncStore>,
    ) -> SuiResult {
        let digest = digests.transaction;

        // TODO: should we suggest that we try peer first?
        let resp = aggregator
            .handle_transaction_and_effects_info_request(digests)
            .await?;

        let cert = resp.certified_transaction.ok_or_else(|| {
            info!(?digest, ?peer, "validator did not return cert");
            SuiError::GenericAuthorityError {
                error: format!("validator did not return cert for {:?}", digest),
            }
        })?;

        let effects = resp.signed_effects.ok_or_else(|| {
            info!(?digest, ?peer, "validator did not return effects");
            SuiError::ByzantineAuthoritySuspicion { authority: peer }
        })?;

        node_sync_store.store_cert_and_effects(&digest, &(cert, effects))?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct NodeSyncDigestHandler {
    _sync_join_handle: Arc<JoinHandle<()>>,
    sender: mpsc::Sender<DigestsMessage>,
}

impl NodeSyncDigestHandler {
    pub fn new<A>(
        state: Arc<AuthorityState>,
        aggregator: Arc<AuthorityAggregator<A>>,
        node_sync_store: Arc<NodeSyncStore>,
    ) -> Self
    where
        A: AuthorityAPI + Send + Sync + 'static + Clone,
    {
        let (sender, receiver) = mpsc::channel(NODE_SYNC_QUEUE_LEN);

        let committee = state.committee.load().deref().clone();
        let sync_state = NodeSyncState {
            committee,
            effects_stake: Mutex::new(EffectsStakeMap::new()),
            state,
            aggregator,
            node_sync_store,
            pending_downloads: Waiter::new(),
            pending_txes: Waiter::new(),
        };

        let _sync_join_handle = Arc::new(sync_state.start(receiver));

        Self {
            _sync_join_handle,
            sender,
        }
    }
}

#[async_trait]
impl<A> DigestHandler<A> for NodeSyncDigestHandler
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    async fn handle_digest(&self, follower: &Follower<A>, digests: ExecutionDigests) -> SuiResult {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(DigestsMessage {
                digests,
                peer: follower.peer_name,
                tx,
            })
            .await
            .map_err(|e| SuiError::GenericAuthorityError {
                error: e.to_string(),
            })?;
        rx.await.map_err(|e| SuiError::GenericAuthorityError {
            error: e.to_string(),
        })?
    }
}

#[cfg(test)]
mod tests {
    // Note: this code is tested end-to-end in full_node_tests.rs

    use sui_types::{
        base_types::{AuthorityName, TransactionEffectsDigest},
        crypto::KeyPair
    };

    use super::EffectsStakeMap;

    fn random_authority_name() -> AuthorityName {
        let key = KeyPair::get_key_pair();
        *key.1.public_key_bytes()
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
