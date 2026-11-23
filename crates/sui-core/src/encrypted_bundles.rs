// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Consensus handler logic for encrypted transaction bundles.
//!
//! At commit `c` the handler accepts bundles that pass deterministic checks and persists them
//! as pending. After commit `c` is pushed, each validator starts deriving the bundle keys from
//! its beacon key shares. Before the handler processes commit `c + 1`, it waits for the keys,
//! waits until every transaction scheduled so far has executed, decrypts the bundles, and
//! validates each decrypted transaction against the executed state. The surviving
//! transactions are scheduled first in commit `c + 1`, in bundle order, with no reordering
//! and no deferral.
//!
//! Decrypted transactions skip the pre-consensus vote, so the prototype accepts only
//! transactions whose validity does not depend on owned-object state: every object input is
//! shared or a package, and gas is paid from the sender's address balance.
//!
//! Known limitation: a validator that executes certified checkpoints ahead of its consensus
//! handler can see packages or shared objects that a later transaction created. Its existence
//! checks can then differ from the other validators'.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use futures::FutureExt;
use futures::future::join_all;
use mysten_common::{ZipDebugEqIteratorExt, fatal};
use parking_lot::Mutex;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::EpochId;
use sui_types::encrypted_bundle::{BeaconPublicKey, BundleKey, EncryptedBundle};
use sui_types::executable_transaction::{
    VerifiedExecutableTransaction, VerifiedExecutableTransactionWithAliases,
};
use sui_types::messages_consensus::EncryptedBundleMessage;
use sui_types::transaction::{
    InputObjectKind, Transaction, TransactionDataAPI, TransactionKey, TransactionKind,
};
use tracing::{debug, info, warn};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::epoch::randomness::DkgStatus;
use crate::execution_cache::{ObjectCacheRead, TransactionCacheRead};

/// The smallest computation charge in gas units (the first computation bucket). A fee
/// transaction pays at least this many units times its gas price.
pub(crate) const MIN_COMPUTATION_UNITS: u64 = 1_000;

/// Tracked keys are pruned once the list reaches this length.
const PRUNE_THRESHOLD: usize = 10_000;

/// Derives bundle keys from the random beacon's key shares.
#[async_trait]
pub trait EncryptedBundleKeys: Send + Sync {
    /// Starts deriving the key for `id_message`. Calling it again for the same identity has
    /// no effect.
    fn request(&self, epoch: EpochId, id_message: Vec<u8>);

    /// Returns the key for `id_message` once it is derived and verified against the beacon
    /// public key.
    async fn wait(&self, epoch: EpochId, id_message: Vec<u8>) -> BundleKey;

    /// The beacon public key of `epoch`, if DKG completed.
    fn beacon_public_key(&self, epoch: EpochId) -> Option<BeaconPublicKey>;
}

/// Per-epoch handler state for encrypted bundles.
pub(crate) struct EncryptedBundleState {
    inner: Mutex<EncryptedBundleStateInner>,
    transaction_cache: Option<Arc<dyn TransactionCacheRead>>,
}

struct EncryptedBundleStateInner {
    /// Bundles accepted in the last processed commit.
    pending: Vec<EncryptedBundle>,
    /// Keys of transactions scheduled by the handler that may not have executed yet.
    scheduled_keys: Vec<TransactionKey>,
}

impl EncryptedBundleState {
    pub(crate) fn new(epoch_store: &AuthorityPerEpochStore) -> Self {
        let pending = epoch_store
            .get_pending_encrypted_bundles()
            .expect("db error");
        if !pending.is_empty() {
            info!(
                "Recovered {} pending encrypted bundles at startup",
                pending.len()
            );
        }
        Self {
            inner: Mutex::new(EncryptedBundleStateInner {
                pending,
                scheduled_keys: vec![],
            }),
            transaction_cache: None,
        }
    }

    pub(crate) fn set_transaction_cache(&mut self, cache: Arc<dyn TransactionCacheRead>) {
        self.transaction_cache = Some(cache);
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.inner.lock().pending.is_empty()
    }

    pub(crate) fn set_pending(&self, bundles: Vec<EncryptedBundle>) {
        self.inner.lock().pending = bundles;
    }

    pub(crate) fn take_pending(&self) -> Vec<EncryptedBundle> {
        std::mem::take(&mut self.inner.lock().pending)
    }

    /// Records keys of scheduled transactions so that a later bundle can wait for them.
    pub(crate) fn record_scheduled(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        keys: impl IntoIterator<Item = TransactionKey>,
    ) {
        if !epoch_store.protocol_config().encrypted_bundles() {
            return;
        }
        let mut inner = self.inner.lock();
        inner.scheduled_keys.extend(keys);
        if inner.scheduled_keys.len() >= PRUNE_THRESHOLD
            && let Some(cache) = &self.transaction_cache
        {
            let before = inner.scheduled_keys.len();
            inner.scheduled_keys.retain(|key| {
                let digest = match key {
                    TransactionKey::Digest(digest) => Some(*digest),
                    _ => epoch_store
                        .notify_read_tx_key_to_digest(std::slice::from_ref(key))
                        .now_or_never()
                        .and_then(|result| result.ok())
                        .and_then(|digests| digests.into_iter().next()),
                };
                !digest.is_some_and(|digest| cache.is_tx_already_executed(&digest))
            });
            debug!(
                "Pruned scheduled keys for encrypted bundles from {before} to {}",
                inner.scheduled_keys.len()
            );
        }
    }

    /// Waits until every transaction scheduled so far has executed. Without a transaction
    /// cache (unit tests) it returns immediately.
    pub(crate) async fn wait_for_execution(&self, epoch_store: &AuthorityPerEpochStore) {
        let keys = std::mem::take(&mut self.inner.lock().scheduled_keys);
        let Some(cache) = &self.transaction_cache else {
            return;
        };
        if keys.is_empty() {
            return;
        }
        let digests = epoch_store
            .notify_read_tx_key_to_digest(&keys)
            .await
            .expect("db error");
        cache
            .notify_read_executed_effects_digests("EncryptedBundles::wait_for_execution", &digests)
            .await;
    }
}

/// Returns whether bundles can be accepted at the given commit. A bundle needs a beacon key,
/// so it is accepted only after DKG succeeded. Without a randomness manager (unit tests) the
/// key provider alone decides.
pub(crate) fn beacon_available(
    epoch_store: &AuthorityPerEpochStore,
    dkg_status: Option<DkgStatus>,
) -> bool {
    if !epoch_store.protocol_config().encrypted_bundles() {
        return false;
    }
    let Some(keys) = epoch_store.encrypted_bundle_keys() else {
        return false;
    };
    match dkg_status {
        Some(DkgStatus::Successful) => true,
        Some(DkgStatus::Pending | DkgStatus::TimedOut) => false,
        None => keys.beacon_public_key(epoch_store.epoch()).is_some(),
    }
}

/// Selects the bundles of one commit. All checks are deterministic. `fee_payments` holds the
/// user transactions that this commit sequences.
pub(crate) fn select_bundles(
    epoch_store: &AuthorityPerEpochStore,
    beacon_available: bool,
    messages: Vec<EncryptedBundleMessage>,
    fee_payments: &[VerifiedExecutableTransactionWithAliases],
) -> Vec<EncryptedBundle> {
    if messages.is_empty() {
        return vec![];
    }
    if !beacon_available {
        warn!(
            "Dropping {} encrypted bundles because the beacon key is not available",
            messages.len()
        );
        return vec![];
    }
    let protocol_config = epoch_store.protocol_config();
    let max_per_commit = protocol_config.encrypted_bundle_max_per_commit() as usize;
    let fee = protocol_config.encrypted_bundle_fee();
    let epoch = epoch_store.epoch();

    let mut used_fee_payments = HashSet::new();
    let mut accepted = Vec::new();
    for EncryptedBundleMessage {
        bundle,
        fee_tx_digest,
    } in messages
    {
        if accepted.len() >= max_per_commit {
            warn!(
                "Dropping encrypted bundle {:?}: commit limit reached",
                bundle.id()
            );
            continue;
        }
        if bundle.epoch != epoch {
            warn!(
                "Dropping encrypted bundle {:?}: epoch {} is not current epoch {epoch}",
                bundle.id(),
                bundle.epoch
            );
            continue;
        }
        if !used_fee_payments.insert(fee_tx_digest) {
            warn!(
                "Dropping encrypted bundle {:?}: fee transaction {fee_tx_digest} pays for another bundle",
                bundle.id()
            );
            continue;
        }
        if let Err(reason) = check_fee_payment(fee_payments, &fee_tx_digest, fee) {
            warn!("Dropping encrypted bundle {:?}: {reason}", bundle.id());
            continue;
        }
        accepted.push(bundle);
    }
    accepted
}

fn check_fee_payment(
    fee_payments: &[VerifiedExecutableTransactionWithAliases],
    fee_tx_digest: &TransactionDigest,
    fee: u64,
) -> Result<(), String> {
    let Some(fee_tx) = fee_payments
        .iter()
        .map(|tx| tx.tx())
        .find(|tx| tx.digest() == fee_tx_digest)
    else {
        return Err(format!(
            "fee transaction {fee_tx_digest} is not sequenced in this commit"
        ));
    };
    let data = fee_tx.data().transaction_data();
    if data.kind().shared_input_objects().next().is_some() {
        return Err(format!("fee transaction {fee_tx_digest} has shared inputs"));
    }
    if data.gas_price().saturating_mul(MIN_COMPUTATION_UNITS) < fee {
        return Err(format!(
            "fee transaction {fee_tx_digest} has gas price {} below the bundle fee",
            data.gas_price()
        ));
    }
    Ok(())
}

/// Requests the keys of `bundles` and waits until all are derived.
pub(crate) async fn derive_keys(
    epoch_store: &AuthorityPerEpochStore,
    bundles: &[EncryptedBundle],
) -> Vec<BundleKey> {
    let epoch = epoch_store.epoch();
    let keys = epoch_store
        .encrypted_bundle_keys()
        .unwrap_or_else(|| fatal!("pending encrypted bundles without a key provider"));
    for bundle in bundles {
        keys.request(epoch, bundle.id_message());
    }
    let derived = join_all(
        bundles
            .iter()
            .map(|bundle| keys.wait(epoch, bundle.id_message())),
    )
    .await;
    if let Some(beacon_pk) = keys.beacon_public_key(epoch) {
        for (bundle, key) in bundles.iter().zip_debug_eq(&derived) {
            // A threshold signature is unique, so every validator derives the same key. A key
            // that does not verify is a local bug, and continuing could fork the validator.
            if bundle.verify_key(&beacon_pk, key).is_err() {
                fatal!(
                    "derived key for encrypted bundle {:?} does not verify",
                    bundle.id()
                );
            }
        }
    }
    derived
}

/// Decrypts a bundle. A bundle that does not decrypt yields no transactions.
pub(crate) fn open_bundle(
    epoch_store: &AuthorityPerEpochStore,
    bundle: &EncryptedBundle,
    key: &BundleKey,
) -> Vec<Transaction> {
    match bundle.open(key) {
        Ok(plaintext) => {
            let max = epoch_store
                .protocol_config()
                .encrypted_bundle_max_transactions() as usize;
            if plaintext.transactions.len() > max {
                warn!(
                    "Dropping encrypted bundle {:?}: {} transactions exceed the limit {max}",
                    bundle.id(),
                    plaintext.transactions.len()
                );
                return vec![];
            }
            plaintext.transactions
        }
        Err(e) => {
            warn!(
                "Encrypted bundle {:?} from {} does not decrypt: {e}",
                bundle.id(),
                bundle.author
            );
            vec![]
        }
    }
}

/// Checks a decrypted transaction. The checks depend only on the protocol config, the epoch
/// and objects that exist after every earlier transaction executed.
pub(crate) fn validate_decrypted_transaction(
    epoch_store: &AuthorityPerEpochStore,
    object_cache: &dyn ObjectCacheRead,
    tx: Transaction,
) -> Result<VerifiedExecutableTransactionWithAliases, String> {
    tx.validity_check(&epoch_store.tx_validity_check_context())
        .map_err(|e| format!("validity check failed: {e}"))?;

    let data = tx.data().transaction_data();
    if !matches!(data.kind(), TransactionKind::ProgrammableTransaction(_)) {
        return Err("only programmable transactions are accepted".to_string());
    }
    if data.uses_randomness() {
        // A randomness transaction could be deferred after it is revealed.
        return Err("randomness transactions are not accepted".to_string());
    }
    if !data.gas().is_empty() || !data.is_gas_paid_from_address_balance() {
        return Err("gas must be paid from the address balance".to_string());
    }
    if data.gas_price() < epoch_store.reference_gas_price() {
        return Err(format!(
            "gas price {} is below the reference gas price",
            data.gas_price()
        ));
    }
    if !data.receiving_objects().is_empty() {
        return Err("receiving arguments are not accepted".to_string());
    }
    let inputs = data
        .input_objects()
        .map_err(|e| format!("invalid inputs: {e}"))?;
    for input in inputs {
        match input {
            InputObjectKind::MovePackage(id) => {
                if object_cache.get_object(&id).is_none() {
                    return Err(format!("package {id} does not exist"));
                }
            }
            InputObjectKind::SharedMoveObject { id, .. } => {
                if object_cache.get_object(&id).is_none() {
                    return Err(format!("shared object {id} does not exist"));
                }
            }
            InputObjectKind::ImmOrOwnedMoveObject(object_ref) => {
                return Err(format!(
                    "owned or immutable input {} is not accepted",
                    object_ref.0
                ));
            }
        }
    }

    let verified = epoch_store
        .verify_transaction_require_no_aliases(tx)
        .map_err(|e| format!("signature verification failed: {e}"))?
        .into_tx();
    Ok(VerifiedExecutableTransactionWithAliases::no_aliases(
        VerifiedExecutableTransaction::new_from_consensus(verified, epoch_store.epoch()),
    ))
}

#[cfg(test)]
#[path = "unit_tests/encrypted_bundles_tests.rs"]
mod encrypted_bundles_tests;
