// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Consensus handler logic for encrypted transactions.
//!
//! An encrypted transaction is a user transaction whose programmable transaction is encrypted
//! to the random beacon key. The sender, gas payment and signature are in plaintext, so the
//! transaction passes the pre-consensus vote like any other.
//!
//! At commit `c` the handler defers the encrypted transactions to the next commit. After commit
//! `c` is pushed, each validator releases its key shares for their identities. Before the
//! handler processes commit `c + 1`, it waits for the keys, waits until every transaction
//! scheduled so far has executed, decrypts the transactions, and validates the decrypted
//! programmable transactions against the executed state. Each encrypted transaction is then
//! scheduled first in commit `c + 1`, with no reordering. A transaction that does not decrypt or
//! fails validation runs no command and only pays gas. The commit prologue records the keys of
//! the others.
//!
//! When a certified checkpoint contains commit `c + 1`, the handler takes the keys from the
//! prologue in that checkpoint instead. Such a checkpoint exists when the node is behind or
//! replays commits after a restart. Its state can then be ahead of commit `c`, and peers may no
//! longer have the keys.
//!
//! The decrypted transaction skips the vote, so its inputs may only be pure values, shared
//! objects, packages and withdrawals from the sender's address balance. Owned objects other
//! than the gas payment are not locked by the vote. The checks that the vote makes for the other
//! inputs are made after decryption instead.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use futures::future::join_all;
use mysten_common::{ZipDebugEqIteratorExt, fatal};
use sui_types::SUI_RANDOMNESS_STATE_OBJECT_ID;
use sui_types::accumulator_root::AccumulatorObjId;
use sui_types::balance::Balance;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::committee::EpochId;
use sui_types::deny_list_v1::check_coin_deny_list_v1_for_addresses;
use sui_types::deny_list_v2::check_coin_deny_list_v2_for_addresses;
use sui_types::encrypted_transaction::{
    EncryptedTransaction, EncryptedTransactionKey, key_from_bytes, key_to_bytes,
};
use sui_types::executable_transaction::{
    VerifiedExecutableTransaction, VerifiedExecutableTransactionWithAliases,
};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_consensus::{ConsensusDeterminedVersionAssignments, Round};
use sui_types::object::{Object, Owner};
use sui_types::storage::ObjectStore;
use sui_types::transaction::{
    CallArg, Command, FundsWithdrawalArg, ObjectArg, TransactionData, TransactionDataAPI,
    TransactionKind, WithAliases, WithdrawFrom,
};
use tokio::sync::oneshot;
use tracing::{info, warn};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::checkpoints::CheckpointStore;
use crate::epoch::randomness::DkgStatus;
use crate::execution_cache::writeback_cache::account_amount_from_object;
use crate::execution_cache::{ObjectCacheRead, TransactionCacheRead};

/// Derives the keys of encrypted transactions from the random beacon's key shares.
pub trait EncryptedTransactionKeys: Send + Sync {
    /// Starts deriving the key for `id_message`, or joins the derivation in progress. The
    /// receiver yields the key once it is verified against the beacon public key.
    fn derive(
        &self,
        epoch: EpochId,
        id_message: Vec<u8>,
    ) -> oneshot::Receiver<EncryptedTransactionKey>;
}

impl EncryptedTransactionKeys for sui_network::randomness::Handle {
    fn derive(
        &self,
        epoch: EpochId,
        id_message: Vec<u8>,
    ) -> oneshot::Receiver<EncryptedTransactionKey> {
        self.derive_encrypted_transaction_key(epoch, id_message)
    }
}

/// The compressed keys of the encrypted transactions that execute their commands in a commit.
pub(crate) type PrologueKeys = Vec<(TransactionDigest, Vec<u8>)>;

pub(crate) fn is_encrypted(transaction: &VerifiedExecutableTransactionWithAliases) -> bool {
    matches!(
        transaction.tx().transaction_data().kind(),
        TransactionKind::EncryptedTransaction(_)
    )
}

fn encrypted_part(
    transaction: &VerifiedExecutableTransaction,
) -> (&EncryptedTransaction, SuiAddress) {
    let data = transaction.transaction_data();
    let TransactionKind::EncryptedTransaction(et) = data.kind() else {
        fatal!("{} is not an encrypted transaction", transaction.digest());
    };
    (et, data.sender())
}

/// Splits the encrypted transactions of one commit into those that are deferred to the next
/// commit and those that run no command. `dkg_status` is `None` only in unit tests. All checks
/// are deterministic.
pub(crate) fn select(
    epoch_store: &AuthorityPerEpochStore,
    dkg_status: Option<DkgStatus>,
    mut transactions: Vec<VerifiedExecutableTransactionWithAliases>,
) -> (
    Vec<VerifiedExecutableTransactionWithAliases>,
    Vec<VerifiedExecutableTransactionWithAliases>,
) {
    let available = epoch_store.accumulators_enabled()
        && epoch_store.encrypted_transaction_keys().is_some()
        && dkg_status.is_none_or(|status| status == DkgStatus::Successful);
    let config = epoch_store.protocol_config();
    // The transactions must be in the checkpoint of their commit's prologue, which a fullnode
    // reads the keys from. At most two system transactions precede them in the commit.
    let max_per_commit = config
        .encrypted_transaction_max_per_commit()
        .min(config.max_transactions_per_checkpoint().saturating_sub(2));
    let accepted = if available {
        transactions.len().min(max_per_commit as usize)
    } else {
        0
    };
    let rejected = transactions.split_off(accepted);
    if !rejected.is_empty() {
        warn!(
            "{} encrypted transactions run no command: available={available}, limit={max_per_commit}",
            rejected.len()
        );
    }
    (transactions, rejected)
}

/// The encrypted transactions of the previous commit.
pub(crate) fn deferred(
    epoch_store: &AuthorityPerEpochStore,
) -> Vec<VerifiedExecutableTransactionWithAliases> {
    if !epoch_store.protocol_config().encrypted_transactions() {
        return vec![];
    }
    epoch_store
        .consensus_output_cache
        .deferred_transactions
        .lock()
        .values()
        .flatten()
        .filter(|transaction| is_encrypted(transaction))
        .cloned()
        .collect()
}

/// Starts deriving the keys of `transactions`, and returns a future that yields them.
pub(crate) fn derive_keys(
    epoch_store: &AuthorityPerEpochStore,
    transactions: &[VerifiedExecutableTransactionWithAliases],
) -> impl Future<Output = Vec<EncryptedTransactionKey>> + use<> {
    let receivers: Vec<_> = transactions
        .iter()
        .map(|transaction| {
            let (et, sender) = encrypted_part(transaction.tx());
            epoch_store
                .encrypted_transaction_keys()
                .unwrap_or_else(|| fatal!("encrypted transaction without a key provider"))
                .derive(epoch_store.epoch(), et.id_message(&sender))
        })
        .collect();
    async move {
        join_all(receivers.into_iter().map(|receiver| async move {
            match receiver.await {
                Ok(key) => key,
                // This node is not in the committee and reads the key from a checkpoint, or
                // the epoch ended and its consensus handler is shutting down.
                Err(_) => std::future::pending().await,
            }
        }))
        .await
    }
}

/// Makes a transaction run no command. No key is released for it.
pub(crate) fn without_commands(
    transaction: VerifiedExecutableTransactionWithAliases,
) -> VerifiedExecutableTransactionWithAliases {
    let (tx, aliases) = transaction.into_inner();
    WithAliases::new(tx.with_encrypted_transaction_plaintext(None), aliases)
}

/// Reads the keys of encrypted transactions from certified checkpoints.
pub(crate) struct CertifiedKeys {
    pub(crate) checkpoint_store: Arc<CheckpointStore>,
    pub(crate) transaction_cache: Arc<dyn TransactionCacheRead>,
}

impl CertifiedKeys {
    /// The keys of the commit at `round`, if this node has a certified checkpoint that contains
    /// the commit's prologue. `next_checkpoint` is the sequence number of the checkpoint that
    /// the handler builds. The commit is in that checkpoint, or in the one after it if the
    /// commit does not fit.
    pub(crate) fn get(
        &self,
        next_checkpoint: CheckpointSequenceNumber,
        round: Round,
    ) -> Option<HashMap<TransactionDigest, Vec<u8>>> {
        [next_checkpoint, next_checkpoint + 1]
            .into_iter()
            .find_map(|seq| self.get_in_checkpoint(seq, round))
    }

    /// Like `get`, but waits for the checkpoint.
    pub(crate) async fn wait(
        &self,
        next_checkpoint: CheckpointSequenceNumber,
        round: Round,
    ) -> HashMap<TransactionDigest, Vec<u8>> {
        for seq in [next_checkpoint, next_checkpoint + 1] {
            self.checkpoint_store
                .notify_read_synced_checkpoint(seq)
                .await;
            if let Some(keys) = self.get_in_checkpoint(seq, round) {
                return keys;
            }
        }
        // The transactions of a certified checkpoint that this node built itself can be missing
        // after a restart. This node has not executed them, so it derives the keys.
        std::future::pending().await
    }

    fn get_in_checkpoint(
        &self,
        seq: CheckpointSequenceNumber,
        round: Round,
    ) -> Option<HashMap<TransactionDigest, Vec<u8>>> {
        let checkpoint = self
            .checkpoint_store
            .get_checkpoint_by_sequence_number(seq)
            .expect("db error")?;
        let contents = self
            .checkpoint_store
            .get_checkpoint_contents(&checkpoint.content_digest)
            .expect("db error")?;
        let digests: Vec<_> = contents.iter().map(|digests| digests.transaction).collect();
        self.transaction_cache
            .multi_get_transaction_blocks(&digests)
            .into_iter()
            .flatten()
            .find_map(|transaction| match transaction.transaction_data().kind() {
                TransactionKind::ConsensusCommitPrologueV4(prologue) if prologue.round == round => {
                    Some(prologue_keys(transaction.transaction_data()).collect())
                }
                _ => None,
            })
    }
}

/// The encrypted transaction keys that `data` records if it is a commit prologue.
fn prologue_keys(data: &TransactionData) -> impl Iterator<Item = (TransactionDigest, Vec<u8>)> {
    let keys = match data.kind() {
        TransactionKind::ConsensusCommitPrologueV4(prologue) => {
            match &prologue.consensus_determined_version_assignments {
                ConsensusDeterminedVersionAssignments::CancelledAndEncryptedTransactions {
                    encrypted_transaction_keys,
                    ..
                } => encrypted_transaction_keys.as_slice(),
                _ => &[],
            }
        }
        _ => &[],
    };
    keys.iter().cloned()
}

/// Sets the programmable transaction that an encrypted transaction executes. A transaction
/// without a key runs no command.
fn decrypt_certified(
    transaction: VerifiedExecutableTransaction,
    keys: &HashMap<TransactionDigest, Vec<u8>>,
) -> VerifiedExecutableTransaction {
    let digest = transaction.digest();
    let plaintext = keys.get(digest).map(|key| {
        let key = key_from_bytes(key).unwrap_or_else(|e| {
            fatal!("invalid key for encrypted transaction {digest} in checkpoint: {e}")
        });
        // Validators decrypted this transaction before they certified the checkpoint.
        let (et, sender) = encrypted_part(&transaction);
        et.open(&sender, &key).unwrap_or_else(|e| {
            fatal!("encrypted transaction {digest} in checkpoint does not decrypt: {e}")
        })
    });
    transaction.with_encrypted_transaction_plaintext(plaintext)
}

/// Sets the programmable transactions of the encrypted transactions in a certified checkpoint.
/// A transaction that executes is in the checkpoint of the prologue that records its key.
pub(crate) fn decrypt_checkpoint_transactions(
    transactions: Vec<VerifiedExecutableTransaction>,
) -> Vec<VerifiedExecutableTransaction> {
    let keys = transactions
        .iter()
        .flat_map(|transaction| prologue_keys(transaction.transaction_data()))
        .collect();
    transactions
        .into_iter()
        .map(|transaction| match transaction.transaction_data().kind() {
            TransactionKind::EncryptedTransaction(_) => decrypt_certified(transaction, &keys),
            _ => transaction,
        })
        .collect()
}

/// Decrypts the encrypted transactions of the previous commit with `keys` from a certified
/// checkpoint. Returns them in commit order with the keys for the prologue.
pub(crate) fn resolve_certified(
    transactions: Vec<VerifiedExecutableTransactionWithAliases>,
    mut keys: HashMap<TransactionDigest, Vec<u8>>,
) -> (Vec<VerifiedExecutableTransactionWithAliases>, PrologueKeys) {
    info!(
        "Decrypting {} encrypted transactions with keys from a certified checkpoint",
        transactions.len()
    );
    let transactions: Vec<_> = transactions
        .into_iter()
        .map(|transaction| {
            let (tx, aliases) = transaction.into_inner();
            WithAliases::new(decrypt_certified(tx, &keys), aliases)
        })
        .collect();
    let prologue_keys = transactions
        .iter()
        .filter_map(|transaction| keys.remove_entry(transaction.tx().digest()))
        .collect();
    (transactions, prologue_keys)
}

/// Decrypts the encrypted transactions of the previous commit with derived `keys`, and validates
/// them against the executed state. Returns them in commit order with the keys for the prologue.
pub(crate) fn resolve_derived(
    epoch_store: &AuthorityPerEpochStore,
    object_cache: &dyn ObjectCacheRead,
    transactions: Vec<VerifiedExecutableTransactionWithAliases>,
    keys: Vec<EncryptedTransactionKey>,
) -> (Vec<VerifiedExecutableTransactionWithAliases>, PrologueKeys) {
    let mut resolver = Resolver {
        epoch_store,
        object_store: LatestObjects(object_cache),
        reserved: BTreeMap::new(),
    };
    let mut prologue_keys = vec![];
    let transactions = transactions
        .into_iter()
        .zip_debug_eq(keys)
        .map(|(transaction, key)| {
            let (tx, aliases) = transaction.into_inner();
            let digest = *tx.digest();
            let tx = match resolver.resolve(&tx, &key) {
                Ok(tx) => {
                    prologue_keys.push((digest, key_to_bytes(&key)));
                    tx
                }
                Err(reason) => {
                    warn!("Encrypted transaction {digest} runs no command: {reason}");
                    tx.with_encrypted_transaction_plaintext(None)
                }
            };
            WithAliases::new(tx, aliases)
        })
        .collect();
    (transactions, prologue_keys)
}

/// The executed state that decrypted transactions are validated against, and the address
/// balance reservations of the encrypted transactions already resolved for this commit.
struct Resolver<'a> {
    epoch_store: &'a AuthorityPerEpochStore,
    object_store: LatestObjects<'a>,
    reserved: BTreeMap<AccumulatorObjId, u128>,
}

impl Resolver<'_> {
    /// Decrypts and validates a transaction, and sets the programmable transaction it executes.
    fn resolve(
        &mut self,
        transaction: &VerifiedExecutableTransaction,
        key: &EncryptedTransactionKey,
    ) -> Result<VerifiedExecutableTransaction, String> {
        let (et, sender) = encrypted_part(transaction);
        let plaintext = et.open(&sender, key).map_err(|e| e.to_string())?;
        let transaction = transaction
            .clone()
            .with_encrypted_transaction_plaintext(Some(plaintext));
        self.validate(transaction.transaction_data())?;
        Ok(transaction)
    }

    /// Checks a transaction whose plaintext is set. The checks depend only on the transaction,
    /// the protocol config, and the state after every earlier transaction executed.
    fn validate(&mut self, data: &TransactionData) -> Result<(), String> {
        // Execution repeats this check and treats a failure as fatal.
        data.validity_check(&self.epoch_store.tx_validity_check_context())
            .map_err(|e| format!("validity check failed: {e}"))?;
        // Only the vote meters the bytecode verifier.
        if data
            .kind()
            .iter_commands()
            .any(|command| matches!(command, Command::Publish(..) | Command::Upgrade(..)))
        {
            return Err("publishing is not accepted".to_string());
        }
        let coin_types = self.check_inputs(data)?;
        self.check_deny_list(data, coin_types)?;
        self.check_funds(data)
    }

    /// Checks the inputs that the vote could not check, and returns the types of the coins among
    /// them that can have a deny list.
    fn check_inputs(&self, data: &TransactionData) -> Result<BTreeSet<String>, String> {
        let transaction = data
            .kind()
            .user_programmable_transaction()
            .expect("the plaintext is set");
        for input in &transaction.inputs {
            match input {
                CallArg::Pure(_)
                | CallArg::Object(ObjectArg::SharedObject { .. })
                | CallArg::FundsWithdrawal(FundsWithdrawalArg {
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
        let mut coin_types = BTreeSet::new();
        for input in transaction.input_objects().map_err(|e| e.to_string())? {
            let id = input.object_id();
            if id == SUI_RANDOMNESS_STATE_OBJECT_ID {
                // A randomness transaction could be deferred after it is revealed.
                return Err("randomness is not accepted".to_string());
            }
            let object = self
                .object_store
                .get_object(&id)
                .ok_or_else(|| format!("object {id} does not exist"))?;
            // A transaction earlier in the commit can transfer a party object.
            if !object.is_package() && !matches!(object.owner, Owner::Shared { .. }) {
                return Err(format!("object {id} is not shared"));
            }
            // Execution repeats this check and treats a failure as fatal.
            sui_transaction_checks::check_one_object(
                &data.sender(),
                input,
                &object,
                /* system_transaction */ false,
                self.epoch_store.protocol_config(),
            )
            .map_err(|e| e.to_string())?;
            if !object.is_gas_coin()
                && let Some(coin_type) = object.coin_type_maybe()
            {
                coin_types.insert(coin_type.to_canonical_string(false));
            }
        }
        Ok(coin_types)
    }

    /// Checks that the sender may spend `coin_types` and the coin types it withdraws. Execution
    /// checks only recipients.
    fn check_deny_list(
        &self,
        data: &TransactionData,
        mut coin_types: BTreeSet<String>,
    ) -> Result<(), String> {
        coin_types.extend(
            data.kind()
                .get_funds_withdrawals()
                .filter_map(|withdrawal| {
                    Balance::maybe_get_balance_type_param(&withdrawal.type_arg.to_type_tag())
                })
                .map(|ty| ty.to_canonical_string(false)),
        );
        let addresses_by_coin_type: BTreeMap<_, _> = coin_types
            .into_iter()
            .map(|coin_type| (coin_type, BTreeSet::from([data.sender()])))
            .collect();
        if addresses_by_coin_type.is_empty() {
            return Ok(());
        }
        if self.epoch_store.coin_deny_list_v1_enabled() {
            check_coin_deny_list_v1_for_addresses(
                addresses_by_coin_type.clone(),
                &self.object_store,
            )
            .map_err(|e| e.to_string())?;
        }
        if self
            .epoch_store
            .protocol_config()
            .enable_coin_deny_list_v2()
        {
            check_coin_deny_list_v2_for_addresses(addresses_by_coin_type, &self.object_store)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Checks that the address balances cover the withdrawals of the transaction, and reserves
    /// them.
    ///
    /// A transaction whose reservations fail in the scheduler pays no gas. The vote cannot
    /// reject a hidden reservation, so without this check a user could make the network derive
    /// keys without paying. The encrypted transactions are the first withdrawals of their commit
    /// and earlier commits have settled, so the scheduler makes the same decision.
    fn check_funds(&mut self, data: &TransactionData) -> Result<(), String> {
        let mut reserved = self.reserved.clone();
        for (account, amount) in data
            .try_process_funds_withdrawals_for_execution(self.epoch_store.get_chain_identifier())
            .map_err(|e| e.to_string())?
        {
            let balance = self
                .object_store
                .get_object(account.inner())
                .map_or(0, |object| account_amount_from_object(&object));
            let total = reserved.entry(account).or_default();
            *total += amount as u128;
            if balance < *total {
                return Err(format!(
                    "account {account:?} has {balance}, but {total} is reserved"
                ));
            }
        }
        self.reserved = reserved;
        Ok(())
    }
}

struct LatestObjects<'a>(&'a dyn ObjectCacheRead);

impl ObjectStore for LatestObjects<'_> {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.0.get_object(object_id)
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.0.get_object_by_key(object_id, version)
    }
}

#[cfg(test)]
#[path = "unit_tests/encrypted_transactions_tests.rs"]
mod encrypted_transactions_tests;
