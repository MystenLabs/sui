// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Derivation of encrypted transaction keys from the random beacon's key shares.
//!
//! The key of an encrypted transaction is the threshold BLS signature on its identity message.
//! A validator sends its partial signatures for an identity only after its consensus handler
//! requests the key, which happens once the commit that contains the transaction is processed.
//! Partial signatures from peers are kept until the local request arrives.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anemo::PeerId;
use fastcrypto_threshold::tbls::ThresholdBls;
use fastcrypto_threshold::types::ThresholdBls12381MinSig;
use mysten_metrics::spawn_monitored_task;
use mysten_network::anemo_ext::NetworkExt;
use serde::{Deserialize, Serialize};
use sui_types::base_types::AuthorityName;
use sui_types::committee::EpochId;
use sui_types::crypto::{RandomnessPartialSignature, RandomnessSignature};
use sui_types::encrypted_transaction::ID_TAG;
use tokio::sync::{OnceCell, oneshot};
use tracing::{debug, warn};

use super::{RandomnessClient, RandomnessEventLoop};

/// Upper bound on the length of an identity message.
pub(super) const MAX_ID_MESSAGE_LEN: usize = 256;
/// Upper bound on the number of identities with buffered peer partial signatures that this
/// node has not requested.
const MAX_UNREQUESTED_IDS: usize = 1_000;
/// Number of rounds a send task keeps sending the full key after the key is known.
const SENDS_AFTER_KEY: usize = 2;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendEncryptedTransactionKeySharesRequest {
    pub(super) epoch: EpochId,
    pub(super) id_message: Vec<u8>,
    // BCS-serialized `RandomnessPartialSignature` values, ordered by share index.
    pub(super) partial_sigs: Vec<Vec<u8>>,
    // The full key, sent instead of partial signatures once the sender has it.
    pub(super) key: Option<RandomnessSignature>,
}

type KeyId = (EpochId, Vec<u8>);

#[derive(Default)]
pub(super) struct EncryptedTransactionKeyState {
    requested: HashMap<KeyId, RequestedKey>,
    unrequested: HashMap<KeyId, HashMap<PeerId, Vec<RandomnessPartialSignature>>>,
    /// Requests for an epoch whose DKG output the event loop does not have yet.
    deferred_requests: Vec<KeyId>,
}

struct RequestedKey {
    partials: HashMap<PeerId, Vec<RandomnessPartialSignature>>,
    key: Arc<OnceCell<RandomnessSignature>>,
    waiters: Vec<oneshot::Sender<RandomnessSignature>>,
    send_task: Option<tokio::task::JoinHandle<()>>,
}

impl RequestedKey {
    fn new() -> Self {
        Self {
            partials: HashMap::new(),
            key: Arc::new(OnceCell::new()),
            waiters: vec![],
            send_task: None,
        }
    }
}

impl EncryptedTransactionKeyState {
    /// Drops the state of earlier epochs.
    pub(super) fn update_epoch(&mut self, epoch: EpochId) {
        self.requested.retain(|(e, _), state| {
            if *e < epoch
                && let Some(task) = state.send_task.take()
            {
                task.abort();
            }
            *e >= epoch
        });
        self.unrequested.retain(|(e, _), _| *e >= epoch);
    }

    pub(super) fn take_deferred_requests(&mut self) -> Vec<KeyId> {
        std::mem::take(&mut self.deferred_requests)
    }
}

impl RandomnessEventLoop {
    pub(super) fn request_encrypted_transaction_key(
        &mut self,
        epoch: EpochId,
        id_message: Vec<u8>,
    ) {
        if !id_message.starts_with(ID_TAG) || id_message.len() > MAX_ID_MESSAGE_LEN {
            warn!("BUG: refusing to derive an encrypted transaction key for a malformed identity");
            return;
        }
        if epoch < self.epoch {
            debug!("ignoring encrypted transaction key request for past epoch {epoch}");
            return;
        }
        let Some(shares) = self
            .dkg_output
            .as_ref()
            .filter(|_| epoch == self.epoch)
            .and_then(|output| output.shares.clone())
        else {
            debug!(
                "deferring encrypted transaction key request until DKG output for epoch {epoch} is available"
            );
            self.encrypted_transaction_keys
                .deferred_requests
                .push((epoch, id_message));
            return;
        };

        let id = (epoch, id_message.clone());
        if self
            .encrypted_transaction_keys
            .requested
            .get(&id)
            .is_some_and(|state| state.send_task.is_some())
        {
            return;
        }
        let early_partials = self
            .encrypted_transaction_keys
            .unrequested
            .remove(&id)
            .unwrap_or_default();
        let own_partials = ThresholdBls12381MinSig::partial_sign_batch(shares.iter(), &id_message);
        let own_peer_id = self.network.peer_id();
        let name = self.name;
        let network = self.network.clone();
        let retry_interval = self.config.partial_signature_retry_interval();
        let authority_info = self.authority_info.clone();

        let state = self
            .encrypted_transaction_keys
            .requested
            .entry(id.clone())
            .or_insert_with(RequestedKey::new);
        state.partials.extend(early_partials);
        state.partials.insert(own_peer_id, own_partials.clone());
        let task = send_encrypted_transaction_key_shares_task(
            name,
            network,
            retry_interval,
            authority_info,
            epoch,
            id_message,
            own_partials,
            state.key.clone(),
        );
        state.send_task = Some(spawn_monitored_task!(task));
        debug!("sending encrypted transaction key shares");
        self.maybe_aggregate_encrypted_transaction_key(&id);
    }

    pub(super) fn wait_encrypted_transaction_key(
        &mut self,
        epoch: EpochId,
        id_message: Vec<u8>,
        sender: oneshot::Sender<RandomnessSignature>,
    ) {
        let state = self
            .encrypted_transaction_keys
            .requested
            .entry((epoch, id_message))
            .or_insert_with(RequestedKey::new);
        match state.key.get() {
            Some(key) => {
                let _ = sender.send(*key);
            }
            None => state.waiters.push(sender),
        }
    }

    pub(super) fn receive_encrypted_transaction_key_shares(
        &mut self,
        peer_id: PeerId,
        request: SendEncryptedTransactionKeySharesRequest,
    ) {
        let SendEncryptedTransactionKeySharesRequest {
            epoch,
            id_message,
            partial_sigs,
            key,
        } = request;
        if epoch != self.epoch
            || !id_message.starts_with(ID_TAG)
            || id_message.len() > MAX_ID_MESSAGE_LEN
        {
            debug!(
                "ignoring encrypted transaction key shares for epoch {epoch} or malformed identity"
            );
            return;
        }
        let Some(vss_pk) = self.dkg_output.as_ref().map(|output| output.vss_pk.c0()) else {
            return;
        };
        let id = (epoch, id_message);

        if let Some(key) = key {
            if ThresholdBls12381MinSig::verify(&vss_pk, &id.1, &key).is_err() {
                warn!("received invalid encrypted transaction key from peer {peer_id}");
                return;
            }
            self.complete_encrypted_transaction_key(&id, key);
            return;
        }

        let Some(expected_share_ids) = self
            .peer_share_ids
            .as_ref()
            .and_then(|ids| ids.get(&peer_id))
        else {
            debug!("received encrypted transaction key shares from unknown peer {peer_id}");
            return;
        };
        let Ok(partials) = partial_sigs
            .iter()
            .map(|bytes| bcs::from_bytes::<RandomnessPartialSignature>(bytes))
            .collect::<Result<Vec<_>, _>>()
        else {
            warn!("failed to deserialize encrypted transaction key shares from peer {peer_id}");
            return;
        };
        if partials.len() != expected_share_ids.len()
            || partials
                .iter()
                .map(|partial| partial.index)
                .ne(expected_share_ids.iter().copied())
        {
            warn!(
                "received encrypted transaction key shares with unexpected share ids from peer {peer_id}"
            );
            return;
        }

        if let Some(state) = self.encrypted_transaction_keys.requested.get_mut(&id) {
            if let Some(key) = state.key.get().copied() {
                // The peer does not have the key yet, for example after a restart.
                send_encrypted_transaction_key_once(&self.network, peer_id, id, key);
                return;
            }
            state.partials.insert(peer_id, partials);
            self.maybe_aggregate_encrypted_transaction_key(&id);
        } else {
            if !self
                .encrypted_transaction_keys
                .unrequested
                .contains_key(&id)
                && self.encrypted_transaction_keys.unrequested.len() >= MAX_UNREQUESTED_IDS
            {
                debug!(
                    "dropping encrypted transaction key shares for an unrequested identity: buffer is full"
                );
                return;
            }
            self.encrypted_transaction_keys
                .unrequested
                .entry(id)
                .or_default()
                .insert(peer_id, partials);
        }
    }

    fn maybe_aggregate_encrypted_transaction_key(&mut self, id: &KeyId) {
        let Some(vss_pk) = self.dkg_output.as_ref().map(|output| output.vss_pk.clone()) else {
            return;
        };
        let threshold = self.aggregation_threshold;
        let Some(state) = self.encrypted_transaction_keys.requested.get_mut(id) else {
            return;
        };
        // Only a locally requested identity is aggregated. The request is the point at which
        // the transaction is known to be committed.
        if state.send_task.is_none() || state.key.initialized() {
            return;
        }
        let aggregate = |partials: &HashMap<PeerId, Vec<RandomnessPartialSignature>>| {
            ThresholdBls12381MinSig::aggregate(threshold, partials.values().flatten())
        };
        let Ok(mut key) = aggregate(&state.partials) else {
            return; // wait for more shares
        };
        if ThresholdBls12381MinSig::verify(&vss_pk.c0(), &id.1, &key).is_err() {
            state.partials.retain(|peer_id, partials| {
                let valid = ThresholdBls12381MinSig::partial_verify_batch(
                    &vss_pk,
                    &id.1,
                    partials.iter(),
                    &mut rand::thread_rng(),
                )
                .is_ok();
                if !valid {
                    warn!("received invalid encrypted transaction key shares from peer {peer_id}");
                }
                valid
            });
            let Ok(retried) = aggregate(&state.partials) else {
                return;
            };
            if ThresholdBls12381MinSig::verify(&vss_pk.c0(), &id.1, &retried).is_err() {
                warn!("encrypted transaction key does not verify after removing invalid shares");
                return;
            }
            key = retried;
        }
        self.complete_encrypted_transaction_key(id, key);
    }

    fn complete_encrypted_transaction_key(&mut self, id: &KeyId, key: RandomnessSignature) {
        let state = self
            .encrypted_transaction_keys
            .requested
            .entry(id.clone())
            .or_insert_with(RequestedKey::new);
        if state.key.set(key).is_err() {
            return;
        }
        state.partials.clear();
        for waiter in state.waiters.drain(..) {
            let _ = waiter.send(key);
        }
        debug!("derived encrypted transaction key");
    }
}

const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Sends this node's partial signatures to every peer until the key is known, then sends the
/// key for a few more rounds so that peers that are behind can complete.
#[allow(clippy::too_many_arguments)]
async fn send_encrypted_transaction_key_shares_task(
    name: AuthorityName,
    network: anemo::Network,
    retry_interval: Duration,
    authority_info: Arc<HashMap<AuthorityName, (PeerId, fastcrypto_threshold::nodes::PartyId)>>,
    epoch: EpochId,
    id_message: Vec<u8>,
    partials: Vec<RandomnessPartialSignature>,
    key: Arc<OnceCell<RandomnessSignature>>,
) {
    let partials: Vec<_> = partials
        .iter()
        .map(|partial| bcs::to_bytes(partial).expect("serialization should not fail"))
        .collect();
    let peers: Vec<_> = authority_info
        .iter()
        .filter(|(peer_name, _)| **peer_name != name)
        .map(|(_, (peer_id, _))| network.waiting_peer(*peer_id))
        .collect();
    let mut sends_after_key = 0;
    loop {
        let known_key = key.get().copied();
        let requests = peers.iter().map(|peer| {
            let mut client = RandomnessClient::new(peer.clone());
            let request = anemo::Request::new(SendEncryptedTransactionKeySharesRequest {
                epoch,
                id_message: id_message.clone(),
                partial_sigs: if known_key.is_none() {
                    partials.clone()
                } else {
                    vec![]
                },
                key: known_key,
            })
            .with_timeout(SEND_TIMEOUT);
            async move {
                if client
                    .send_encrypted_transaction_key_shares(request)
                    .await
                    .is_err()
                {
                    debug!("failed to send encrypted transaction key shares");
                }
            }
        });
        futures::future::join_all(requests).await;
        if known_key.is_some() {
            sends_after_key += 1;
            if sends_after_key >= SENDS_AFTER_KEY {
                return;
            }
        }
        tokio::time::sleep(retry_interval).await;
    }
}

fn send_encrypted_transaction_key_once(
    network: &anemo::Network,
    peer_id: PeerId,
    (epoch, id_message): KeyId,
    key: RandomnessSignature,
) {
    let peer = network.waiting_peer(peer_id);
    spawn_monitored_task!(async move {
        let mut client = RandomnessClient::new(peer);
        let request = anemo::Request::new(SendEncryptedTransactionKeySharesRequest {
            epoch,
            id_message,
            partial_sigs: vec![],
            key: Some(key),
        })
        .with_timeout(SEND_TIMEOUT);
        if client
            .send_encrypted_transaction_key_shares(request)
            .await
            .is_err()
        {
            debug!("failed to send encrypted transaction key to peer {peer_id}");
        }
    });
}
