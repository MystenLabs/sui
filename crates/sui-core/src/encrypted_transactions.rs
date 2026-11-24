// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Consensus handler logic for encrypted transactions.
//!
//! An encrypted transaction is a user transaction whose programmable transaction is encrypted
//! to the random beacon key. The sender, gas payment and signature are in plaintext, so the
//! transaction passes the pre-consensus vote like any other.
//!
//! At commit `c` the handler takes the encrypted transactions out of the schedule and persists
//! them as pending. After commit `c` is pushed, each validator releases its key shares for
//! their identities. Before the handler processes commit `c + 1`, it waits for the keys, waits
//! until every transaction scheduled so far has executed, decrypts the transactions, and
//! validates the decrypted programmable transactions against the executed state. Each
//! encrypted transaction is then scheduled first in commit `c + 1`, with no reordering and no
//! deferral. A transaction that does not decrypt or fails validation runs no command and only
//! pays gas. The commit prologue records the keys, so nodes that replay checkpoints can
//! decrypt the transactions.
//!
//! The decrypted transaction skips the vote, so its inputs may only be pure values, shared
//! objects, packages and withdrawals from the sender's address balance. Owned objects other
//! than the gas payment are not locked by the vote. The checks that the vote makes for
//! withdrawals are made after decryption instead.
//!
//! Known limitation: a validator that executes certified checkpoints ahead of its consensus
//! handler can see packages or shared objects that a later transaction created. Its existence
//! checks can then differ from the other validators'.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use futures::FutureExt;
use futures::future::join_all;
use mysten_common::{ZipDebugEqIteratorExt, fatal};
use parking_lot::Mutex;
use sui_types::accumulator_root::AccumulatorObjId;
use sui_types::balance::Balance;
use sui_types::base_types::{
    ConsensusObjectSequenceKey, FullObjectID, ObjectID, SequenceNumber, SuiAddress,
    TransactionDigest,
};
use sui_types::committee::EpochId;
use sui_types::deny_list_v1::check_coin_deny_list_v1_for_addresses;
use sui_types::deny_list_v2::check_coin_deny_list_v2_for_addresses;
use sui_types::encrypted_transaction::{
    BeaconPublicKey, EncryptedTransaction, EncryptedTransactionKey, EncryptedTransactionResolution,
    key_from_bytes, key_to_bytes,
};
use sui_types::executable_transaction::{
    VerifiedExecutableTransaction, VerifiedExecutableTransactionWithAliases,
};
use sui_types::messages_consensus::ConsensusDeterminedVersionAssignments;
use sui_types::object::Object;
use sui_types::storage::{InputKey, ObjectStore};
use sui_types::transaction::{
    CallArg, FundsWithdrawalArg, InputObjectKind, ObjectArg, TransactionData, TransactionDataAPI,
    TransactionKey, TransactionKind, VerifiedTransaction, WithAliases, WithdrawFrom,
};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_DENY_LIST_OBJECT_ID, SUI_RANDOMNESS_STATE_OBJECT_ID,
};
use tracing::{debug, info, warn};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::epoch::randomness::DkgStatus;
use crate::execution_cache::writeback_cache::account_amount_from_object;
use crate::execution_cache::{ObjectCacheRead, TransactionCacheRead};
use crate::randomness_round_receiver::RandomnessRoundReceiverHandle;

/// Tracked keys are pruned once the list reaches this length.
const PRUNE_THRESHOLD: usize = 10_000;

/// Derives the keys of encrypted transactions from the random beacon's key shares.
#[async_trait]
pub trait EncryptedTransactionKeys: Send + Sync {
    /// Starts deriving the key for `id_message`. Calling it again for the same identity has
    /// no effect.
    fn request(&self, epoch: EpochId, id_message: Vec<u8>);

    /// Returns the key for `id_message` once it is derived and verified against the beacon
    /// public key.
    async fn wait(&self, epoch: EpochId, id_message: Vec<u8>) -> EncryptedTransactionKey;

    /// The beacon public key of `epoch`, if DKG completed.
    fn beacon_public_key(&self, epoch: EpochId) -> Option<BeaconPublicKey>;
}

/// Derives keys over the randomness network service.
pub struct NetworkEncryptedTransactionKeys {
    network: sui_network::randomness::Handle,
    beacon: Arc<RandomnessRoundReceiverHandle>,
}

impl NetworkEncryptedTransactionKeys {
    pub fn new(
        network: sui_network::randomness::Handle,
        beacon: Arc<RandomnessRoundReceiverHandle>,
    ) -> Self {
        Self { network, beacon }
    }
}

#[async_trait]
impl EncryptedTransactionKeys for NetworkEncryptedTransactionKeys {
    fn request(&self, epoch: EpochId, id_message: Vec<u8>) {
        self.network
            .request_encrypted_transaction_key(epoch, id_message);
    }

    async fn wait(&self, epoch: EpochId, id_message: Vec<u8>) -> EncryptedTransactionKey {
        match self
            .network
            .wait_encrypted_transaction_key(epoch, id_message)
            .await
        {
            Ok(key) => key,
            // The event loop dropped the request because the epoch ended. The consensus
            // handler of that epoch is shutting down, so the wait never completes.
            Err(_) => std::future::pending().await,
        }
    }

    fn beacon_public_key(&self, _epoch: EpochId) -> Option<BeaconPublicKey> {
        // The receiver handle holds the key of the current epoch only.
        self.beacon.public_key()
    }
}

/// Per-epoch handler state for encrypted transactions.
pub(crate) struct EncryptedTransactionState {
    inner: Mutex<EncryptedTransactionStateInner>,
    transaction_cache: Option<Arc<dyn TransactionCacheRead>>,
}

struct EncryptedTransactionStateInner {
    /// Encrypted transactions accepted in the last processed commit.
    pending: Vec<VerifiedExecutableTransactionWithAliases>,
    /// Keys of transactions scheduled by the handler that may not have executed yet.
    scheduled_keys: Vec<TransactionKey>,
}

impl EncryptedTransactionState {
    pub(crate) fn new(epoch_store: &AuthorityPerEpochStore) -> Self {
        let pending = epoch_store
            .get_pending_encrypted_transactions()
            .expect("db error");
        if !pending.is_empty() {
            info!(
                "Recovered {} pending encrypted transactions at startup",
                pending.len()
            );
        }
        Self {
            inner: Mutex::new(EncryptedTransactionStateInner {
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

    pub(crate) fn set_pending(&self, transactions: Vec<VerifiedExecutableTransactionWithAliases>) {
        self.inner.lock().pending = transactions;
    }

    pub(crate) fn take_pending(&self) -> Vec<VerifiedExecutableTransactionWithAliases> {
        std::mem::take(&mut self.inner.lock().pending)
    }

    /// Records keys of scheduled transactions so that a later commit can wait for them.
    pub(crate) fn record_scheduled(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        keys: impl IntoIterator<Item = TransactionKey>,
    ) {
        if !epoch_store.protocol_config().encrypted_transactions() {
            return;
        }
        let mut inner = self.inner.lock();
        // The prologue placeholder is replaced by the real prologue transaction before
        // scheduling, so its key is never mapped to a digest. The prologue runs before the
        // other transactions of its commit, so waiting for those covers it.
        inner.scheduled_keys.extend(
            keys.into_iter()
                .filter(|key| !matches!(key, TransactionKey::ConsensusCommitPrologue(..))),
        );
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
                "Pruned scheduled keys for encrypted transactions from {before} to {}",
                inner.scheduled_keys.len()
            );
        }
    }

    /// Waits until every transaction scheduled so far has executed and its balance changes
    /// have settled. Without a transaction cache (unit tests) it returns immediately.
    pub(crate) async fn wait_for_execution(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        object_cache: &dyn ObjectCacheRead,
    ) {
        let keys = std::mem::take(&mut self.inner.lock().scheduled_keys);
        let Some(cache) = &self.transaction_cache else {
            return;
        };
        let digests = epoch_store
            .notify_read_tx_key_to_digest(&keys)
            .await
            .expect("db error");
        cache
            .notify_read_executed_effects_digests(
                "EncryptedTransactions::wait_for_execution",
                &digests,
            )
            .await;
        if let Some(key) = accumulator_root_key(epoch_store) {
            // The settlement of the last commit writes this version of the accumulator root.
            let (id, initial_shared_version) = key;
            object_cache
                .notify_read_input_objects(
                    &[InputKey::VersionedObject {
                        id: FullObjectID::Consensus((id, initial_shared_version)),
                        version: next_version(epoch_store, object_cache, key),
                    }],
                    &HashSet::new(),
                    epoch_store.epoch(),
                )
                .await;
        }
    }
}

pub(crate) fn is_encrypted(transaction: &VerifiedExecutableTransactionWithAliases) -> bool {
    matches!(
        transaction.tx().transaction_data().kind(),
        TransactionKind::EncryptedTransaction(_)
    )
}

fn encrypted_part(
    transaction: &VerifiedExecutableTransactionWithAliases,
) -> (&EncryptedTransaction, SuiAddress) {
    let data = transaction.tx().transaction_data();
    let TransactionKind::EncryptedTransaction(et) = data.kind() else {
        fatal!(
            "{} is not an encrypted transaction",
            transaction.tx().digest()
        );
    };
    (et, data.sender())
}

/// Returns whether encrypted transactions can be decrypted at the given commit. A key needs
/// the beacon, so it exists only after DKG succeeded. Without a randomness manager (unit
/// tests) the key provider alone decides.
pub(crate) fn beacon_available(
    epoch_store: &AuthorityPerEpochStore,
    dkg_status: Option<DkgStatus>,
) -> bool {
    let Some(keys) = epoch_store.encrypted_transaction_keys() else {
        return false;
    };
    match dkg_status {
        Some(DkgStatus::Successful) => true,
        Some(DkgStatus::Pending | DkgStatus::TimedOut) => false,
        None => keys.beacon_public_key(epoch_store.epoch()).is_some(),
    }
}

/// Splits the encrypted transactions of one commit into those whose keys are released and
/// those that run no command. All checks are deterministic.
pub(crate) fn select(
    epoch_store: &AuthorityPerEpochStore,
    beacon_available: bool,
    transactions: Vec<VerifiedExecutableTransactionWithAliases>,
) -> (
    Vec<VerifiedExecutableTransactionWithAliases>,
    Vec<VerifiedExecutableTransactionWithAliases>,
) {
    let max_per_commit = epoch_store
        .protocol_config()
        .encrypted_transaction_max_per_commit_as_option()
        .map(|max| max as usize)
        .unwrap_or(usize::MAX);
    let epoch = epoch_store.epoch();
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for transaction in transactions {
        let (et, _) = encrypted_part(&transaction);
        let digest = *transaction.tx().digest();
        if !beacon_available {
            warn!(
                "Encrypted transaction {digest} runs no command: the beacon key is not available"
            );
            rejected.push(transaction);
        } else if et.epoch != epoch {
            warn!(
                "Encrypted transaction {digest} runs no command: epoch {} is not current epoch {epoch}",
                et.epoch
            );
            rejected.push(transaction);
        } else if accepted.len() >= max_per_commit {
            warn!("Encrypted transaction {digest} runs no command: commit limit reached");
            rejected.push(transaction);
        } else {
            accepted.push(transaction);
        }
    }
    (accepted, rejected)
}

/// The identity messages whose keys the validator releases after the commit is pushed.
pub(crate) fn id_messages(
    transactions: &[VerifiedExecutableTransactionWithAliases],
) -> Vec<Vec<u8>> {
    transactions
        .iter()
        .map(|transaction| {
            let (et, sender) = encrypted_part(transaction);
            et.id_message(&sender)
        })
        .collect()
}

/// Makes a transaction run no command. No key is released for it.
pub(crate) fn without_commands(
    transaction: VerifiedExecutableTransactionWithAliases,
) -> (
    VerifiedExecutableTransactionWithAliases,
    EncryptedTransactionResolution,
) {
    let resolution = EncryptedTransactionResolution {
        transaction_digest: *transaction.tx().digest(),
        key: vec![],
        executes: false,
    };
    let (tx, aliases) = transaction.into_inner();
    (
        WithAliases::new(tx.with_encrypted_transaction_plaintext(None), aliases),
        resolution,
    )
}

/// Requests the keys of `transactions` and waits until all are derived.
pub(crate) async fn derive_keys(
    epoch_store: &AuthorityPerEpochStore,
    transactions: &[VerifiedExecutableTransactionWithAliases],
) -> Vec<EncryptedTransactionKey> {
    let epoch = epoch_store.epoch();
    let keys = epoch_store
        .encrypted_transaction_keys()
        .unwrap_or_else(|| fatal!("pending encrypted transactions without a key provider"));
    let id_messages = id_messages(transactions);
    for id_message in &id_messages {
        keys.request(epoch, id_message.clone());
    }
    let derived = join_all(
        id_messages
            .iter()
            .map(|id_message| keys.wait(epoch, id_message.clone())),
    )
    .await;
    if let Some(beacon_pk) = keys.beacon_public_key(epoch) {
        for (transaction, key) in transactions.iter().zip_debug_eq(&derived) {
            let (et, sender) = encrypted_part(transaction);
            // A threshold signature is unique, so every validator derives the same key. A key
            // that does not verify is a local bug, and continuing could fork the validator.
            if et.verify_key(&beacon_pk, &sender, key).is_err() {
                fatal!(
                    "derived key for encrypted transaction {} does not verify",
                    transaction.tx().digest()
                );
            }
        }
    }
    derived
}

/// The executed state that decrypted transactions are validated against, and the address
/// balance reservations of the encrypted transactions already resolved for this commit.
///
/// Object versions come from consensus version assignment, so a validator that executed
/// checkpoints ahead of its consensus handler reads the same deny list and balances as the
/// others.
pub(crate) struct Validator<'a> {
    epoch_store: &'a AuthorityPerEpochStore,
    object_cache: &'a dyn ObjectCacheRead,
    /// The version of the accumulator root that the next commit reads.
    accumulator_version: Option<SequenceNumber>,
    /// The version of the deny list that the next commit reads.
    deny_list_version: Option<SequenceNumber>,
    reserved: BTreeMap<AccumulatorObjId, u128>,
}

impl<'a> Validator<'a> {
    pub(crate) fn new(
        epoch_store: &'a AuthorityPerEpochStore,
        object_cache: &'a dyn ObjectCacheRead,
    ) -> Self {
        let accumulator_version = accumulator_root_key(epoch_store)
            .map(|key| next_version(epoch_store, object_cache, key));
        let deny_list_version = epoch_store
            .epoch_start_config()
            .coin_deny_list_obj_initial_shared_version()
            .map(|version| {
                next_version(
                    epoch_store,
                    object_cache,
                    (SUI_DENY_LIST_OBJECT_ID, version),
                )
            });
        Self {
            epoch_store,
            object_cache,
            accumulator_version,
            deny_list_version,
            reserved: BTreeMap::new(),
        }
    }

    /// Decrypts and validates a transaction, and sets the programmable transaction it executes.
    pub(crate) fn resolve(
        &mut self,
        transaction: VerifiedExecutableTransactionWithAliases,
        key: &EncryptedTransactionKey,
    ) -> (
        VerifiedExecutableTransactionWithAliases,
        EncryptedTransactionResolution,
    ) {
        let digest = *transaction.tx().digest();
        let (et, sender) = encrypted_part(&transaction);
        let decrypted = et.open(&sender, key).map_err(|e| e.to_string());
        let (mut tx, aliases) = transaction.into_inner();
        let result = decrypted.and_then(|pt| {
            tx = tx.clone().with_encrypted_transaction_plaintext(Some(pt));
            self.validate(tx.transaction_data())
        });
        let executes = result.is_ok();
        if let Err(reason) = result {
            warn!("Encrypted transaction {digest} runs no command: {reason}");
            tx = tx.with_encrypted_transaction_plaintext(None);
        }
        self.reserve(tx.transaction_data());
        let resolution = EncryptedTransactionResolution {
            transaction_digest: digest,
            key: key_to_bytes(key),
            executes,
        };
        (WithAliases::new(tx, aliases), resolution)
    }

    /// Checks a transaction whose plaintext is set. The checks depend only on the transaction,
    /// the protocol config, and objects that exist after every earlier transaction executed.
    fn validate(&self, data: &TransactionData) -> Result<(), String> {
        // Execution repeats this check and treats a failure as fatal.
        data.validity_check(&self.epoch_store.tx_validity_check_context())
            .map_err(|e| format!("validity check failed: {e}"))?;
        let transaction = data
            .kind()
            .user_programmable_transaction()
            .expect("the plaintext is set");
        for input in &transaction.inputs {
            match input {
                CallArg::Pure(_) => (),
                CallArg::Object(ObjectArg::SharedObject { .. }) => (),
                CallArg::FundsWithdrawal(FundsWithdrawalArg {
                    withdraw_from: WithdrawFrom::Sender,
                    ..
                }) => (),
                other => {
                    return Err(format!(
                        "only pure, shared object and sender withdrawal inputs are accepted, \
                         found {other:?}"
                    ));
                }
            }
        }
        let inputs = transaction
            .input_objects()
            .map_err(|e| format!("invalid inputs: {e}"))?;
        for input in inputs {
            match input {
                InputObjectKind::MovePackage(id) => {
                    if self.object_cache.get_object(&id).is_none() {
                        return Err(format!("package {id} does not exist"));
                    }
                }
                InputObjectKind::SharedMoveObject { id, .. } => {
                    if id == SUI_RANDOMNESS_STATE_OBJECT_ID {
                        // A randomness transaction could be deferred after it is revealed.
                        return Err("randomness is not accepted".to_string());
                    }
                    if self.object_cache.get_object(&id).is_none() {
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
        if data.has_funds_withdrawals() {
            self.check_deny_list(data)?;
            self.check_funds(data)?;
        }
        Ok(())
    }

    /// Checks that the sender may spend the coin types it withdraws. The vote makes this check
    /// for plaintext withdrawals, and execution checks only recipients.
    fn check_deny_list(&self, data: &TransactionData) -> Result<(), String> {
        let Some(version) = self.deny_list_version else {
            return Ok(());
        };
        let sender = data.sender();
        let addresses_by_coin_type: BTreeMap<_, _> = data
            .kind()
            .get_funds_withdrawals()
            .filter_map(|withdrawal| {
                Balance::maybe_get_balance_type_param(&withdrawal.type_arg.to_type_tag())
            })
            .map(|ty| (ty.to_canonical_string(false), BTreeSet::from([sender])))
            .collect();
        let store = ObjectsAtVersion {
            object_cache: self.object_cache,
            version,
        };
        if self.epoch_store.coin_deny_list_v1_enabled() {
            check_coin_deny_list_v1_for_addresses(addresses_by_coin_type.clone(), &store)
                .map_err(|e| e.to_string())?;
        }
        if self
            .epoch_store
            .protocol_config()
            .enable_coin_deny_list_v2()
        {
            check_coin_deny_list_v2_for_addresses(addresses_by_coin_type, &store)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Checks that the address balances cover the reservations of the transaction.
    ///
    /// A transaction whose reservations fail in the scheduler pays no gas. The vote cannot
    /// reject a hidden reservation, so without this check a user could make the network derive
    /// keys without paying. The encrypted transactions are the first withdrawals at this
    /// accumulator version and earlier commits have settled, so the scheduler makes the same
    /// decision.
    fn check_funds(&self, data: &TransactionData) -> Result<(), String> {
        let version = self
            .accumulator_version
            .ok_or("accumulators are not enabled")?;
        let reservations = data
            .try_process_funds_withdrawals_for_execution(self.epoch_store.get_chain_identifier())
            .map_err(|e| e.to_string())?;
        for (account, amount) in reservations {
            let balance = self
                .object_cache
                .find_object_lt_or_eq_version(*account.inner(), version)
                .map_or(0, |object| account_amount_from_object(&object));
            let reserved = self.reserved.get(&account).copied().unwrap_or_default();
            if balance < reserved + amount as u128 {
                return Err(format!(
                    "account {account:?} has {balance}, of which {reserved} is reserved, but \
                     {amount} is requested"
                ));
            }
        }
        Ok(())
    }

    /// Records the reservations of a resolved transaction.
    fn reserve(&mut self, data: &TransactionData) {
        if !data.has_funds_withdrawals() {
            return;
        }
        for (account, amount) in
            data.process_funds_withdrawals_for_execution(self.epoch_store.get_chain_identifier())
        {
            *self.reserved.entry(account).or_default() += amount as u128;
        }
    }
}

fn accumulator_root_key(
    epoch_store: &AuthorityPerEpochStore,
) -> Option<ConsensusObjectSequenceKey> {
    if !epoch_store.accumulators_enabled() {
        return None;
    }
    let version = epoch_store
        .epoch_start_config()
        .accumulator_root_obj_initial_shared_version()?;
    Some((SUI_ACCUMULATOR_ROOT_OBJECT_ID, version))
}

/// The version of a consensus object that the next transaction which uses it reads.
fn next_version(
    epoch_store: &AuthorityPerEpochStore,
    object_cache: &dyn ObjectCacheRead,
    key: ConsensusObjectSequenceKey,
) -> SequenceNumber {
    epoch_store
        .get_or_init_next_object_versions(&[key], object_cache)
        .expect("db error")[&key]
}

/// Reads objects at their latest version that is at most `version`.
struct ObjectsAtVersion<'a> {
    object_cache: &'a dyn ObjectCacheRead,
    version: SequenceNumber,
}

impl ObjectStore for ObjectsAtVersion<'_> {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.object_cache
            .find_object_lt_or_eq_version(*object_id, self.version)
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.object_cache.get_object_by_key(object_id, version)
    }
}

/// Adds the encrypted transaction keys that a commit prologue records to `keys`. Returns
/// whether `data` is a commit prologue.
fn add_prologue_keys(
    data: &TransactionData,
    keys: &mut HashMap<TransactionDigest, EncryptedTransactionResolution>,
) -> bool {
    if let TransactionKind::ConsensusCommitPrologueV4(prologue) = data.kind()
        && let ConsensusDeterminedVersionAssignments::CancelledAndEncryptedTransactions {
            encrypted_transactions,
            ..
        } = &prologue.consensus_determined_version_assignments
    {
        for resolution in encrypted_transactions {
            keys.insert(resolution.transaction_digest, resolution.clone());
        }
    }
    data.is_consensus_commit_prologue()
}

fn needs_key(transaction: &VerifiedExecutableTransaction) -> bool {
    matches!(
        transaction.transaction_data().kind(),
        TransactionKind::EncryptedTransaction(et) if et.decrypted().is_none()
    )
}

/// Sets the programmable transactions of the encrypted transactions in a checkpoint from the
/// keys that commit prologues record.
///
/// The encrypted transactions of a commit directly follow its prologue. When the commit spans
/// several checkpoints, the prologue is in an earlier checkpoint. `earlier_checkpoints` yields
/// the transactions of the earlier checkpoints of the epoch, latest first. It is read only when
/// a key is missing, and only up to the first checkpoint that contains a prologue, because that
/// prologue is the latest one before this checkpoint.
///
/// A transaction without a key is left as it is: it executes only if this node has not
/// executed it yet, and execution then fails loudly.
pub(crate) fn decrypt_checkpoint_transactions(
    transactions: Vec<VerifiedExecutableTransaction>,
    earlier_checkpoints: impl IntoIterator<Item = Vec<Arc<VerifiedTransaction>>>,
) -> Vec<VerifiedExecutableTransaction> {
    let mut keys = HashMap::new();
    for transaction in &transactions {
        add_prologue_keys(transaction.transaction_data(), &mut keys);
    }
    let key_missing = transactions
        .iter()
        .any(|transaction| needs_key(transaction) && !keys.contains_key(transaction.digest()));
    if key_missing {
        for checkpoint in earlier_checkpoints {
            let mut has_prologue = false;
            for transaction in &checkpoint {
                has_prologue |= add_prologue_keys(transaction.transaction_data(), &mut keys);
            }
            if has_prologue {
                break;
            }
        }
    }

    transactions
        .into_iter()
        .map(|transaction| {
            if !needs_key(&transaction) {
                return transaction;
            }
            let digest = *transaction.digest();
            let Some(resolution) = keys.get(&digest) else {
                debug!("No key known for encrypted transaction {digest}");
                return transaction;
            };
            let data = transaction.transaction_data();
            let TransactionKind::EncryptedTransaction(et) = data.kind() else {
                unreachable!("needs_key checked the kind");
            };
            let decrypted = resolution.executes.then(|| {
                let key = key_from_bytes(&resolution.key).unwrap_or_else(|e| {
                    fatal!("invalid key for encrypted transaction {digest} in checkpoint: {e}")
                });
                // Validators decrypted this transaction before they certified the checkpoint.
                et.open(&data.sender(), &key).unwrap_or_else(|e| {
                    fatal!("encrypted transaction {digest} in checkpoint does not decrypt: {e}")
                })
            });
            transaction.with_encrypted_transaction_plaintext(decrypted)
        })
        .collect()
}

#[cfg(test)]
#[path = "unit_tests/encrypted_transactions_tests.rs"]
mod encrypted_transactions_tests;
