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
/// Upper bound on the number of identities that this node has not requested and for which it
/// buffers the partial signatures of one peer.
const MAX_UNREQUESTED_IDS_PER_PEER: usize = 1_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendEncryptedTransactionKeySharesRequest {
    pub(super) epoch: EpochId,
    pub(super) id_message: Vec<u8>,
    // BCS-serialized `RandomnessPartialSignature` values, ordered by share index.
    pub(super) partial_sigs: Vec<Vec<u8>>,
    // The full key, sent instead of partial signatures in reply to a peer that does not have it.
    pub(super) key: Option<RandomnessSignature>,
}

type KeyId = (EpochId, Vec<u8>);

#[derive(Default)]
pub(super) struct EncryptedTransactionKeyState {
    requested: HashMap<KeyId, RequestedKey>,
    unrequested: HashMap<PeerId, HashMap<KeyId, Vec<RandomnessPartialSignature>>>,
}

struct RequestedKey {
    partials: HashMap<PeerId, Vec<RandomnessPartialSignature>>,
    key: Arc<OnceCell<RandomnessSignature>>,
    waiters: Vec<oneshot::Sender<RandomnessSignature>>,
    send_task: tokio::task::JoinHandle<()>,
}

impl Drop for RequestedKey {
    fn drop(&mut self) {
        self.send_task.abort();
    }
}

impl EncryptedTransactionKeyState {
    /// Drops the state of earlier epochs.
    pub(super) fn update_epoch(&mut self, epoch: EpochId) {
        self.requested.retain(|(e, _), _| *e >= epoch);
        self.unrequested.clear();
    }
}

impl RandomnessEventLoop {
    pub(super) fn derive_encrypted_transaction_key(
        &mut self,
        epoch: EpochId,
        id_message: Vec<u8>,
        waiter: oneshot::Sender<RandomnessSignature>,
    ) {
        if !id_message.starts_with(ID_TAG) || id_message.len() > MAX_ID_MESSAGE_LEN {
            warn!("BUG: refusing to derive an encrypted transaction key for a malformed identity");
            return;
        }
        // The epoch's DKG output arrives before the first request, because the consensus handler
        // accepts encrypted transactions only after DKG. Nodes that are not in the committee
        // never get it, and read keys from checkpoints.
        let Some(dkg_output) = self.dkg_output.as_ref().filter(|_| epoch == self.epoch) else {
            debug!("not deriving an encrypted transaction key without the DKG output of {epoch}");
            return;
        };

        let id = (epoch, id_message);
        if let Some(state) = self.encrypted_transaction_keys.requested.get_mut(&id) {
            match state.key.get() {
                Some(key) => {
                    let _ = waiter.send(*key);
                }
                None => state.waiters.push(waiter),
            }
            return;
        }

        // A validator without shares sends an empty list. It receives the partial signatures of
        // its peers, or the key from a peer that has it.
        let own_partials =
            ThresholdBls12381MinSig::partial_sign_batch(dkg_output.shares.iter().flatten(), &id.1);
        let mut partials: HashMap<_, _> = self
            .encrypted_transaction_keys
            .unrequested
            .iter_mut()
            .filter_map(|(peer_id, ids)| Some((*peer_id, ids.remove(&id)?)))
            .collect();
        partials.insert(self.network.peer_id(), own_partials.clone());
        let key = Arc::new(OnceCell::new());
        let task = send_encrypted_transaction_key_shares_task(
            self.name,
            self.network.clone(),
            self.config.partial_signature_retry_interval(),
            self.authority_info.clone(),
            id.clone(),
            own_partials,
            key.clone(),
        );
        let send_task = spawn_monitored_task!(task);
        self.encrypted_transaction_keys.requested.insert(
            id.clone(),
            RequestedKey {
                partials,
                key,
                waiters: vec![waiter],
                send_task,
            },
        );
        debug!("sending encrypted transaction key shares");
        self.maybe_aggregate_encrypted_transaction_key(&id);
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
        let requested = self.encrypted_transaction_keys.requested.get(&id);
        let known_key = requested.and_then(|state| state.key.get().copied());

        if let Some(key) = key {
            // A peer sends the key only in reply to this node's request. Several peers reply, and
            // keys after the first valid one are not verified.
            if requested.is_none() || known_key.is_some() {
                return;
            }
            if ThresholdBls12381MinSig::verify(&vss_pk, &id.1, &key).is_err() {
                warn!("received invalid encrypted transaction key from peer {peer_id}");
                return;
            }
            self.complete_encrypted_transaction_key(&id, key);
            return;
        }

        if let Some(key) = known_key {
            // The peer may not have the key yet, for example after a restart.
            send_encrypted_transaction_key_once(&self.network, peer_id, id, key);
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
            state.partials.insert(peer_id, partials);
            self.maybe_aggregate_encrypted_transaction_key(&id);
            return;
        }
        let buffered = self
            .encrypted_transaction_keys
            .unrequested
            .entry(peer_id)
            .or_default();
        if buffered.len() >= MAX_UNREQUESTED_IDS_PER_PEER && !buffered.contains_key(&id) {
            debug!("dropping encrypted transaction key shares from peer {peer_id}: buffer is full");
            return;
        }
        buffered.insert(id, partials);
    }

    fn maybe_aggregate_encrypted_transaction_key(&mut self, id: &KeyId) {
        let Some(vss_pk) = self.dkg_output.as_ref().map(|output| output.vss_pk.clone()) else {
            return;
        };
        let threshold = self.aggregation_threshold;
        let Some(state) = self.encrypted_transaction_keys.requested.get_mut(id) else {
            return;
        };
        if state.key.initialized() {
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
        let Some(state) = self.encrypted_transaction_keys.requested.get_mut(id) else {
            return;
        };
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

/// Sends this node's partial signatures to every peer until the key is known. It sends them at
/// least once, because other validators may need them.
async fn send_encrypted_transaction_key_shares_task(
    name: AuthorityName,
    network: anemo::Network,
    retry_interval: Duration,
    authority_info: Arc<HashMap<AuthorityName, (PeerId, fastcrypto_threshold::nodes::PartyId)>>,
    (epoch, id_message): KeyId,
    partials: Vec<RandomnessPartialSignature>,
    key: Arc<OnceCell<RandomnessSignature>>,
) {
    let request = SendEncryptedTransactionKeySharesRequest {
        epoch,
        id_message,
        partial_sigs: partials
            .iter()
            .map(|partial| bcs::to_bytes(partial).expect("serialization should not fail"))
            .collect(),
        key: None,
    };
    let peers: Vec<_> = authority_info
        .iter()
        .filter(|(peer_name, _)| **peer_name != name)
        .map(|(_, (peer_id, _))| network.waiting_peer(*peer_id))
        .collect();
    loop {
        let sends = peers.iter().map(|peer| {
            let mut client = RandomnessClient::new(peer.clone());
            let request = anemo::Request::new(request.clone()).with_timeout(SEND_TIMEOUT);
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
        futures::future::join_all(sends).await;
        tokio::time::sleep(retry_interval).await;
        if key.initialized() {
            return;
        }
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
