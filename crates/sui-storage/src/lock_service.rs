// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! lock_service is a single-threaded atomic Sui Object locking service.
//! Object locks have three phases:
//! 1. (object has no lock, doesn't exist)
//! 2. None (object has an empty lock, but exists. The state when a new object is created)
//! 3. Locked (object has a Transaction digest in the lock, so it's only usable by that transaction)
//!
//! The cycle goes from None (object creation) -> Locked -> deleted/doesn't exist after a Transaction.
//!
//! Lock state is persisted in RocksDB and should be consistent.
//!
//! Communication with the lock service happens through two MPSC queue/channels.
//! One channel is for atomic writes/mutates (init, acquire, remove), the other is for reads.
//! This allows reads to proceed without being blocked on writes.

use futures::channel::oneshot;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::{debug, error, info, trace, warn};
use typed_store::rocks::{DBBatch, DBMap, DBOptions};
use typed_store::traits::Map;
use typed_store::traits::TypedStoreDebug;
use typed_store_derive::DBMapUtils;

use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber, TransactionDigest};
use sui_types::batch::TxSequenceNumber;
use sui_types::committee::EpochId;
use sui_types::error::{SuiError, SuiResult};
use sui_types::{fp_bail, fp_ensure};

use crate::{block_on_future_in_sim, default_db_options};

/// Commands to send to the LockService (for mutating lock state)
// TODO: use smallvec as an optimization
#[derive(Debug)]
enum LockServiceCommands {
    Acquire {
        epoch: EpochId,
        refs: Vec<ObjectRef>,
        tx_digest: TransactionDigest,
        resp: oneshot::Sender<SuiResult>,
    },
    Initialize {
        refs: Vec<ObjectRef>,
        is_force_reset: bool,
        resp: oneshot::Sender<SuiResult>,
    },
    SequenceTransaction {
        tx: TransactionDigest,
        seq: TxSequenceNumber,
        inputs: Vec<ObjectRef>,
        outputs: Vec<ObjectRef>,
        resp: oneshot::Sender<SuiResult<TxSequenceNumber>>,
    },
    CreateLocksForGenesisObjects {
        objects: Vec<ObjectRef>,
        resp: oneshot::Sender<SuiResult>,
    },
}

type SuiLockResult = SuiResult<ObjectLockInfo>;

/// Queries to the LockService state
#[derive(Debug)]
enum LockServiceQueries {
    GetLock {
        object: ObjectRef,
        resp: oneshot::Sender<SuiLockResult>,
    },
    CheckLocksExist {
        objects: Vec<ObjectRef>,
        resp: oneshot::Sender<SuiResult>,
    },
    GetTxSequence {
        tx: TransactionDigest,
        resp: oneshot::Sender<Result<Option<TxSequenceNumber>, SuiError>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectLockInfo {
    requested_object_ref_lock_details: Option<Option<LockDetails>>,
    latest_object_ref: ObjectRef,
}

impl ObjectLockInfo {
    /// If the given ObjectRef record is initailized or locked.
    /// If true, the object version is ready for being used in transactions
    /// If false, the object is currently locked at another version
    pub fn is_initialized_or_locked_at_given_version(&self) -> bool {
        self.requested_object_ref_lock_details.is_some()
    }

    /// If the given ObjectRef is locked by a certain transaction.
    /// Returns false if the object is currently locked at another version,
    ///     or the record is initialized but not locked by any transaction.
    pub fn is_locked_at_given_version(&self) -> bool {
        matches!(self.requested_object_ref_lock_details, Some(Some(_)))
    }

    /// Get the transaction that locks the given ObjectRef.
    /// Returns None if the object is currently locked at another version
    /// (namely `is_initialized_or_locked_at_given_version` returns false)
    pub fn tx_locks_given_version(&self) -> Option<&LockDetails> {
        if let Some(Some(details)) = &self.requested_object_ref_lock_details {
            Some(details)
        } else {
            None
        }
    }

    /// Returns the ObjectRef record currently initialized/locked for this object
    pub fn current_object_record(&self) -> ObjectRef {
        self.latest_object_ref
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockDetails {
    pub epoch: EpochId,
    pub tx_digest: TransactionDigest,
}

/// Inner LockService implementation that does single threaded database accesses.  Cannot be
/// used publicly, must be wrapped in a LockService to control access.
#[derive(Clone, DBMapUtils)]
pub struct LockServiceImpl {
    /// This is a map between object references of currently active objects that can be mutated,
    /// and the transaction that they are lock on for use by this specific authority. Where an object
    /// lock exists for an object version, but no transaction has been seen using it the lock is set
    /// to None. The safety of consistent broadcast depend on each honest authority never changing
    /// the lock once it is set. After a certificate for this object is processed it can be
    /// forgotten.
    #[default_options_override_fn = "transaction_lock_table_default_config"]
    transaction_lock: DBMap<ObjectRef, Option<LockDetails>>,

    /// The semantics of transaction_lock ensure that certificates are always processed
    /// in causal order - that is, certificates naturally form a partial order. tx_sequence
    /// records a total ordering among all processed certificates (which is naturally local
    /// to this authority).
    #[default_options_override_fn = "tx_sequence_table_default_config"]
    tx_sequence: DBMap<TransactionDigest, TxSequenceNumber>,
}

// These functions are used to initialize the DB tables
fn transaction_lock_table_default_config() -> DBOptions {
    default_db_options(None, None).1
}
fn tx_sequence_table_default_config() -> DBOptions {
    default_db_options(None, None).1
}

// TODO: Create method needs to make sure only one instance or thread of this is running per authority
// If not for multiple authorities per process, it should really be one per process.
impl LockServiceImpl {
    fn get_tx_sequence(&self, tx: TransactionDigest) -> SuiResult<Option<TxSequenceNumber>> {
        self.tx_sequence.get(&tx).map_err(SuiError::StorageError)
    }

    /// Gets ObjectLockInfo that represents state of lock on an object.
    /// Returns SuiError::ObjectNotFound if cannot find lock record for this object
    fn get_lock(&self, obj_ref: ObjectRef) -> SuiLockResult {
        Ok(
            if let Some(lock_info) = self
                .transaction_lock
                .get(&obj_ref)
                .map_err(SuiError::StorageError)?
            {
                ObjectLockInfo {
                    requested_object_ref_lock_details: Some(lock_info),
                    latest_object_ref: obj_ref,
                }
            } else {
                ObjectLockInfo {
                    requested_object_ref_lock_details: None,
                    latest_object_ref: self.get_latest_lock_for_object_id(obj_ref.0)?,
                }
            },
        )
    }

    /// Checks multiple object locks exist.
    /// Returns SuiError::ObjectNotFound if cannot find lock record for at least one of the objects.
    /// Returns SuiError::ObjectVersionUnavailableForConsumption if at least one object lock is not initialized
    ///     at the given version.
    fn locks_exist(&self, objects: &[ObjectRef]) -> SuiResult {
        let locks = self.transaction_lock.multi_get(objects)?;
        for (lock, obj_ref) in locks.into_iter().zip(objects) {
            if lock.is_none() {
                let latest_lock = self.get_latest_lock_for_object_id(obj_ref.0)?;
                fp_bail!(SuiError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: *obj_ref,
                    current_version: latest_lock.1
                });
            }
        }
        debug!(?objects, "locks_exist: all locks do exist");
        Ok(())
    }

    fn create_locks_for_genesis_objects(&self, objects: &[ObjectRef]) -> SuiResult {
        let write_batch = self.transaction_lock.batch();
        let write_batch = self.initialize_locks_impl(write_batch, objects, false)?;
        write_batch.write()?;
        Ok(())
    }

    fn sequence_transaction_impl(
        &self,
        tx: TransactionDigest,
        seq: TxSequenceNumber,
        inputs: &[ObjectRef],
        outputs: &[ObjectRef],
    ) -> SuiResult<TxSequenceNumber> {
        // Assert that this tx has not been sequenced under a different number.
        match self.get_tx_sequence(tx)? {
            // tx has already been sequenced
            Some(prev_seq) => Ok(prev_seq),

            None => {
                // Assert that the tx locks exist.
                //
                // Note that the locks may not be set to this particular tx:
                //
                // 1. Lock existence prevents re-execution of old certs when objects have been
                //    upgraded
                // 2. Not all validators lock, just 2f+1, so transaction should proceed regardless
                //    (But the lock should exist which means previous transactions finished)
                // 3. Equivocation possible (different TX) but as long as 2f+1 approves current TX
                //    its fine
                //
                // TODO: it should be impossible for this to fail unless the store has been
                // corrupted. Remove this check when we feel confident enough.
                if let Err(e) = self.locks_exist(inputs) {
                    error!(tx_digest = ?tx, "Locks did not exist for unsequenced transaction! \
                                         possible data store corruption");
                    Err(e)
                } else {
                    // Locks exist - safe to assign the sequence number and initialize new locks.
                    // This step must be done atomically.
                    //
                    // If it was not atomic, we would have to choose either:
                    //
                    // 1. sequence assigned before locks initialized.
                    //
                    //    This would mean that, during recovery, we would have to retry lock
                    //    creation if sequence had already been assigned. But there are two reasons
                    //    the locks might not exist: We could have failed before creating them, or
                    //    they could have been deleted by a subsequent transaction in which case we
                    //    should not recreate them. We can't easily distinguish these two cases,
                    //    so this does not work.
                    //
                    // 2. locks initialized before sequence assigned.
                    //
                    //    This would allow subsequent transactions to run before we assign the
                    //    sequence number to this transaction, which could result in transactions
                    //    being sequenced out of causal order.
                    let write_batch = self.tx_sequence.batch();
                    let write_batch =
                        write_batch.insert_batch(&self.tx_sequence, std::iter::once((tx, seq)))?;

                    let write_batch = self.initialize_locks_impl(write_batch, outputs, false)?;
                    write_batch.write()?;
                    Ok(seq)
                }
            }
        }
    }

    fn sequence_transaction(
        &self,
        tx: TransactionDigest,
        seq: TxSequenceNumber,
        // The objects that we must have locks for.
        inputs: &[ObjectRef],
        // The objects that we must create new locks for.
        outputs: &[ObjectRef],
    ) -> SuiResult<TxSequenceNumber> {
        let seq = self.sequence_transaction_impl(tx, seq, inputs, outputs)?;

        // delete_locks need not be atomic with sequence_transaction_impl
        // - lock deletion is idempotent
        // - this tx will not read the locks again if re-executed, as it will fail the
        //   has-it-been-sequenced check.
        // - no other certificate can exist that reads these locks.
        self.delete_locks(inputs)?;
        Ok(seq)
    }

    /// Acquires a lock for a transaction on the given objects if they have all been initialized previously
    /// to None state.  It is also OK if they have been set to the same transaction.
    /// The locks are all set to the given transaction digest.
    /// Returns SuiError::ObjectNotFound if no lock record can be found for one of the objects.
    /// Returns SuiError::ObjectVersionUnavailableForConsumption if one of the objects is not locked at the given version.
    /// Returns SuiError::ObjectLockConflict if one of the objects is locked by a different transaction in the same epoch.
    /// Returns SuiError::ObjectLockedAtFutureEpoch if one of the objects is locked in a future epoch (bug).
    fn acquire_locks(
        &self,
        epoch: EpochId,
        owned_input_objects: &[ObjectRef],
        tx_digest: TransactionDigest,
    ) -> SuiResult {
        debug!(?tx_digest, ?owned_input_objects, "acquire_locks");
        let mut locks_to_write = Vec::new();
        let locks = self.transaction_lock.multi_get(owned_input_objects)?;

        for ((i, lock), obj_ref) in locks.iter().enumerate().zip(owned_input_objects) {
            // The object / version must exist, and therefore lock initialized.
            let lock = lock.as_ref();
            if lock.is_none() {
                let latest_lock = self.get_latest_lock_for_object_id(obj_ref.0)?;
                fp_bail!(SuiError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: *obj_ref,
                    current_version: latest_lock.1
                });
            }
            // Safe to unwrap as it is checked above
            let lock = lock.unwrap();

            if let Some(LockDetails {
                epoch: previous_epoch,
                tx_digest: previous_tx_digest,
            }) = lock
            {
                fp_ensure!(
                    &epoch >= previous_epoch,
                    SuiError::ObjectLockedAtFutureEpoch {
                        obj_refs: owned_input_objects.to_vec(),
                        locked_epoch: *previous_epoch,
                        new_epoch: epoch,
                    }
                );
                // Lock already set to different transaction from the same epoch.
                // If the lock is set in a previous epoch, it's ok to override it.
                if previous_epoch == &epoch && previous_tx_digest != &tx_digest {
                    // TODO: add metrics here
                    debug!(prev_tx_digest =? previous_tx_digest,
                          cur_tx_digest =? tx_digest,
                          "Conflicting transaction! Lock state changed in unexpected way");
                    return Err(SuiError::ObjectLockConflict {
                        obj_ref: *obj_ref,
                        pending_transaction: *previous_tx_digest,
                    });
                }
                if &epoch == previous_epoch {
                    // Exactly the same epoch and same transaction, nothing to lock here.
                    continue;
                } else {
                    debug!(prev_epoch =? previous_epoch, cur_epoch =? epoch, ?tx_digest, "Overriding an old lock from previous epoch");
                    // Fall through and override the old lock.
                }
            }
            let obj_ref = owned_input_objects[i];
            locks_to_write.push((obj_ref, Some(LockDetails { epoch, tx_digest })));
        }

        if !locks_to_write.is_empty() {
            trace!(?locks_to_write, "Writing locks");
            self.transaction_lock
                .batch()
                .insert_batch(&self.transaction_lock, locks_to_write)?
                .write()?;
        }

        Ok(())
    }

    /// Initialize a lock to None (but exists) for a given list of ObjectRefs.
    /// Returns SuiError::ObjectLockAlreadyInitialized if the lock already exists and is locked to a transaction
    fn initialize_locks_impl(
        &self,
        write_batch: DBBatch,
        objects: &[ObjectRef],
        is_force_reset: bool,
    ) -> SuiResult<DBBatch> {
        debug!(?objects, "initialize_locks");
        // Use a multiget for efficiency
        let locks = self.transaction_lock.multi_get(objects)?;

        if !is_force_reset {
            // If any locks exist and are not None, return errors for them
            let existing_locks: Vec<ObjectRef> = locks
                .iter()
                .zip(objects)
                .filter_map(|(lock_opt, objref)| {
                    lock_opt.clone().flatten().map(|_tx_digest| *objref)
                })
                .collect();
            if !existing_locks.is_empty() {
                info!(
                    ?existing_locks,
                    "Cannot initialize locks because some exist already"
                );
                return Err(SuiError::ObjectLockAlreadyInitialized {
                    refs: existing_locks,
                });
            }
        }

        let write_batch = write_batch.insert_batch(
            &self.transaction_lock,
            objects.iter().map(|obj_ref| (obj_ref, None)),
        )?;

        Ok(write_batch)
    }

    fn initialize_locks(&self, objects: &[ObjectRef], is_force_reset: bool) -> SuiResult {
        let write_batch = self.transaction_lock.batch();
        let write_batch = self.initialize_locks_impl(write_batch, objects, is_force_reset)?;
        write_batch.write()?;
        Ok(())
    }

    /// Removes locks for a given list of ObjectRefs.
    fn delete_locks(&self, objects: &[ObjectRef]) -> SuiResult {
        debug!(?objects, "delete_locks");
        self.transaction_lock.multi_remove(objects)?;
        Ok(())
    }

    /// Returns SuiError::ObjectNotFound if no lock records found for this object.
    pub fn get_latest_lock_for_object_id(&self, object_id: ObjectID) -> SuiResult<ObjectRef> {
        let mut iterator = self
            .transaction_lock
            .iter()
            // Make the max possible entry for this object ID.
            .skip_prior_to(&(object_id, SequenceNumber::MAX, ObjectDigest::MAX))?;
        Ok(iterator
            .next()
            .and_then(|value| {
                if value.0 .0 == object_id {
                    Some(value)
                } else {
                    None
                }
            })
            .ok_or(SuiError::ObjectNotFound {
                object_id,
                version: None,
            })?
            .0)
    }

    /// Loop to continuously process mutating commands in a single thread from async senders.
    /// It terminates when the sender drops, which usually is when the containing data store is dropped.
    fn run_command_loop(&self, mut receiver: Receiver<LockServiceCommands>) {
        debug!("LockService command processing loop started");
        // NOTE: we use blocking_recv() as its faster than using regular async recv() with awaits in a loop
        while let Some(msg) = receiver.blocking_recv() {
            match msg {
                LockServiceCommands::Acquire {
                    epoch,
                    refs,
                    tx_digest,
                    resp,
                } => {
                    let res = self.acquire_locks(epoch, &refs, tx_digest);
                    if let Err(_e) = resp.send(res) {
                        warn!("Could not respond to sender, sender dropped!");
                    }
                }
                LockServiceCommands::Initialize {
                    refs,
                    is_force_reset,
                    resp,
                } => {
                    if let Err(_e) = resp.send(self.initialize_locks(&refs, is_force_reset)) {
                        warn!("Could not respond to sender, sender dropped!");
                    }
                }
                LockServiceCommands::SequenceTransaction {
                    tx,
                    seq,
                    inputs,
                    outputs,
                    resp,
                } => {
                    if let Err(_e) =
                        resp.send(self.sequence_transaction(tx, seq, &inputs, &outputs))
                    {
                        warn!("Could not respond to sender!");
                    }
                }
                LockServiceCommands::CreateLocksForGenesisObjects { objects, resp } => {
                    if let Err(_e) = resp.send(self.create_locks_for_genesis_objects(&objects)) {
                        warn!("Could not respond to sender!");
                    }
                }
            }
        }
        info!("LockService command loop stopped, the sender on other end hung up/dropped");
    }

    /// Loop to continuously process queries in a single thread
    fn run_queries_loop(&self, mut receiver: Receiver<LockServiceQueries>) {
        debug!("LockService queries processing loop started");
        while let Some(msg) = receiver.blocking_recv() {
            match msg {
                LockServiceQueries::GetLock { object, resp } => {
                    if let Err(_e) = resp.send(self.get_lock(object)) {
                        warn!("Could not respond to sender!");
                    }
                }
                LockServiceQueries::CheckLocksExist { objects, resp } => {
                    if let Err(_e) = resp.send(self.locks_exist(&objects)) {
                        warn!("Could not respond to sender, sender dropped!");
                    }
                }
                LockServiceQueries::GetTxSequence { tx, resp } => {
                    if let Err(_e) = resp.send(self.get_tx_sequence(tx)) {
                        warn!("Could not respond to sender, sender dropped!");
                    }
                }
            }
        }
        info!("LockService queries loop stopped, the sender on other end hung up/dropped");
    }
}

const LOCKSERVICE_QUEUE_LEN: usize = 500;

/// Atomic Sui Object locking service.
/// Primary abstraction is an atomic op to acquire a lock on a given set of objects.
/// Atomicity relies on single threaded loop and only one instance per authority.
#[derive(Clone)]
pub struct LockService {
    inner: Arc<LockServiceInner>,
}

struct LockServiceInner {
    sender: Option<Sender<LockServiceCommands>>,
    query_sender: Option<Sender<LockServiceQueries>>,
    run_command_loop: Option<JoinHandle<()>>,
    run_queries_loop: Option<JoinHandle<()>>,
}

impl LockServiceInner {
    #[inline]
    fn sender(&self) -> &Sender<LockServiceCommands> {
        self.sender
            .as_ref()
            .expect("LockServiceInner should not have been dropped yet")
    }

    #[inline]
    fn query_sender(&self) -> &Sender<LockServiceQueries> {
        self.query_sender
            .as_ref()
            .expect("LockServiceInner should not have been dropped yet")
    }
}

impl Drop for LockServiceInner {
    fn drop(&mut self) {
        debug!("Begin Dropping LockService");

        // Take the two Senders and immediately drop them. This will prompt the two threads
        // "run_command_loop" and "run_queries_loop" to terminate so that we can join the threads.
        self.sender.take();
        self.query_sender.take();
        self.run_command_loop
            .take()
            .expect("run_command_loop thread should not have already been joined")
            .join()
            .unwrap();
        self.run_queries_loop
            .take()
            .expect("run_queries_loop thread should not have already been joined")
            .join()
            .unwrap();

        debug!("End Dropping LockService");
    }
}

impl LockService {
    /// Create a new instance of LockService.  For now, the caller has to guarantee only one per data store -
    /// namely each SuiDataStore creates its own LockService.
    pub fn new(path: PathBuf, db_options: Option<Options>) -> Result<Self, SuiError> {
        let inner_service = LockServiceImpl::open_tables_read_write(path, db_options, None);

        // Now, create a sync channel and spawn a thread
        let (sender, receiver) = channel(LOCKSERVICE_QUEUE_LEN);
        let inner2 = inner_service.clone();
        let run_command_loop = std::thread::spawn(move || {
            inner2.run_command_loop(receiver);
        });

        let (q_sender, q_receiver) = channel(LOCKSERVICE_QUEUE_LEN);
        let run_queries_loop = std::thread::spawn(move || {
            inner_service.run_queries_loop(q_receiver);
        });

        Ok(Self {
            inner: Arc::new(LockServiceInner {
                sender: Some(sender),
                query_sender: Some(q_sender),
                run_command_loop: Some(run_command_loop),
                run_queries_loop: Some(run_queries_loop),
            }),
        })
    }

    /// Acquires a lock for a transaction on the given objects if they have all been initialized previously
    /// to None state.  It is also OK if they have been set to the same transaction.
    /// The locks are all set to the given transaction digest.
    /// Otherwise, SuiError(TransactionLockDoesNotExist, ConflictingTransaction) is returned.
    /// Note that this method sends a message to inner LockService implementation and waits for a response
    pub async fn acquire_locks(
        &self,
        epoch: EpochId,
        refs: Vec<ObjectRef>,
        tx_digest: TransactionDigest,
    ) -> SuiResult {
        block_on_future_in_sim(async move {
            let (os_sender, os_receiver) = oneshot::channel::<SuiResult>();
            self.inner
                .sender()
                .send(LockServiceCommands::Acquire {
                    epoch,
                    refs,
                    tx_digest,
                    resp: os_sender,
                })
                .await
                .expect("Could not send message to inner LockService");
            os_receiver
                .await
                .expect("Response from lockservice was cancelled, should not happen!")
        })
        .await
    }

    /// Initialize a lock to None (but exists) for a given list of ObjectRefs.
    /// If `is_force_reset` is true, we initialize them regardless of their existing state.
    /// Otherwise, if the lock already exists and is locked to a transaction, then return TransactionLockExists
    /// Only the gateway could set is_force_reset to true.
    pub async fn initialize_locks(&self, refs: &[ObjectRef], is_force_reset: bool) -> SuiResult {
        block_on_future_in_sim(async move {
            let (os_sender, os_receiver) = oneshot::channel::<SuiResult>();
            self.inner
                .sender()
                .send(LockServiceCommands::Initialize {
                    refs: Vec::from(refs),
                    is_force_reset,
                    resp: os_sender,
                })
                .await
                .expect("Could not send message to inner LockService");
            os_receiver
                .await
                .expect("Response from lockservice was cancelled, should not happen!")
        })
        .await
    }

    /// Returns the state of a single lock.
    /// * None - lock does not exist and is not initialized
    /// * Some(None) - lock exists and is initialized, but not locked to a particular transaction
    /// * Some(Some(tx_digest)) - lock exists and set to transaction
    pub async fn get_lock(&self, object: ObjectRef) -> SuiLockResult {
        block_on_future_in_sim(async move {
            let (os_sender, os_receiver) = oneshot::channel::<SuiLockResult>();
            self.inner
                .query_sender()
                .send(LockServiceQueries::GetLock {
                    object,
                    resp: os_sender,
                })
                .await
                .expect("Could not send message to inner LockService");
            os_receiver
                .await
                .expect("Response from lockservice was cancelled, should not happen!")
        })
        .await
    }

    pub async fn get_tx_sequence(
        &self,
        tx: TransactionDigest,
    ) -> SuiResult<Option<TxSequenceNumber>> {
        block_on_future_in_sim(async move {
            let (os_sender, os_receiver) =
                oneshot::channel::<Result<Option<TxSequenceNumber>, SuiError>>();
            self.inner
                .query_sender()
                .send(LockServiceQueries::GetTxSequence {
                    tx,
                    resp: os_sender,
                })
                .await
                .expect("Could not send message to inner LockService");
            os_receiver
                .await
                .expect("Response from lockservice was cancelled, should not happen!")
        })
        .await
    }

    pub async fn create_locks_for_genesis_objects(&self, objects: Vec<ObjectRef>) -> SuiResult {
        block_on_future_in_sim(async move {
            let (os_sender, os_receiver) = oneshot::channel::<SuiResult>();
            self.inner
                .sender()
                .send(LockServiceCommands::CreateLocksForGenesisObjects {
                    objects,
                    resp: os_sender,
                })
                .await
                .expect("Could not send message to inner LockService");
            os_receiver
                .await
                .expect("Response from lockservice was cancelled, should not happen!")
        })
        .await
    }

    /// Attempts to sequence the given tx. Sequencing consists of:
    ///
    /// 1. Check if the tx has been previously sequenced under a different sequence number,
    ///    if so, return it.
    /// 2. If not, atomically record the tx->sequence number assignment and initialize locks for
    ///    the output objects of this tx.
    /// 3. Delete locks for the tx input objects (this step is not atomic).
    ///
    /// Return value:
    /// - The sequence number that was assigned to tx - may differ from the `seq` parameter if the
    ///   tx was previously sequenced.
    pub async fn sequence_transaction(
        &self,
        tx: TransactionDigest,
        seq: TxSequenceNumber,
        inputs: Vec<ObjectRef>,
        outputs: Vec<ObjectRef>,
    ) -> SuiResult<TxSequenceNumber> {
        block_on_future_in_sim(async move {
            let (os_sender, os_receiver) = oneshot::channel::<SuiResult<TxSequenceNumber>>();
            self.inner
                .sender()
                .send(LockServiceCommands::SequenceTransaction {
                    tx,
                    seq,
                    inputs,
                    outputs,
                    resp: os_sender,
                })
                .await
                .expect("Could not send message to inner LockService");
            os_receiver
                .await
                .expect("Response from lockservice was cancelled, should not happen!")
        })
        .await
    }

    /// Checks multiple object locks exist.
    /// Returns Err(TransactionLockDoesNotExist) if at least one object lock is not initialized.
    pub async fn locks_exist(&self, objects: Vec<ObjectRef>) -> SuiResult {
        block_on_future_in_sim(async move {
            let (os_sender, os_receiver) = oneshot::channel::<SuiResult>();
            self.inner
                .query_sender()
                .send(LockServiceQueries::CheckLocksExist {
                    objects,
                    resp: os_sender,
                })
                .await
                .expect("Could not send message to inner LockService");
            os_receiver
                .await
                .expect("Response from lockservice was cancelled, should not happen!")
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::join_all;
    use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, TransactionDigest};
    use sui_types::error::SuiError;

    use pretty_assertions::assert_eq;

    fn init_lockservice_db() -> LockServiceImpl {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        std::fs::create_dir(&path).unwrap();
        LockServiceImpl::open_tables_read_write(path, None, None)
    }

    fn init_lockservice() -> LockService {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        std::fs::create_dir(&path).unwrap();

        LockService::new(path, None).expect("Could not create LockService")
    }

    // Test acquire_locks() and initialize_locks()
    #[tokio::test]
    async fn test_lockdb_acquire_init_multiple() {
        let ls = init_lockservice_db();

        let ref1: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());
        let ref2: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());
        let ref3: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());

        let tx1 = TransactionDigest::random();
        let tx2 = TransactionDigest::random();

        // Should not be able to acquire lock for uninitialized locks
        assert_eq!(
            ls.acquire_locks(0, &[ref1, ref2], tx1),
            Err(SuiError::ObjectNotFound {
                object_id: ref1.0,
                version: None
            })
        );
        assert_eq!(
            ls.get_lock(ref1),
            Err(SuiError::ObjectNotFound {
                object_id: ref1.0,
                version: None
            })
        );

        // Initialize 2 locks
        ls.initialize_locks(&[ref1, ref2], false /* is_force_reset */)
            .unwrap();
        let lock_info = ls.get_lock(ref2).unwrap();
        assert_eq!(lock_info.current_object_record(), ref2);
        assert!(lock_info.is_initialized_or_locked_at_given_version());
        assert!(!lock_info.is_locked_at_given_version());
        assert!(lock_info.tx_locks_given_version().is_none());

        assert_eq!(ls.locks_exist(&[ref1, ref2]), Ok(()));

        // Should not be able to acquire lock if not all objects initialized
        assert_eq!(
            ls.acquire_locks(0, &[ref1, ref2, ref3], tx1),
            Err(SuiError::ObjectNotFound {
                object_id: ref3.0,
                version: None
            })
        );

        // Should be able to acquire lock if all objects initialized
        ls.acquire_locks(0, &[ref1, ref2], tx1).unwrap();
        let lock_info = ls.get_lock(ref2).unwrap();
        assert_eq!(lock_info.current_object_record(), ref2);
        assert!(lock_info.is_initialized_or_locked_at_given_version());
        assert!(lock_info.is_locked_at_given_version());
        assert_eq!(
            lock_info.tx_locks_given_version(),
            Some(&LockDetails {
                epoch: 0,
                tx_digest: tx1
            })
        );

        // Should be able to check locks exist for ref1 and ref2, but not others
        assert_eq!(ls.locks_exist(&[ref1, ref2]), Ok(()));
        assert_eq!(
            ls.locks_exist(&[ref2, ref3]),
            Err(SuiError::ObjectNotFound {
                object_id: ref3.0,
                version: None
            })
        );

        // Should get ObjectLockAlreadyInitialized because ref2's lock entry already exists
        assert!(matches!(
            ls.initialize_locks(&[ref2, ref3], false /* is_force_reset */),
            Err(SuiError::ObjectLockAlreadyInitialized { .. })
        ));

        ls.initialize_locks(&[ref3], false /* is_force_reset */)
            .unwrap();
        // Should not be able to acquire lock because ref2 is locked to a different transaction
        assert!(matches!(
            ls.acquire_locks(0, &[ref2, ref3], tx2),
            Err(SuiError::ObjectLockConflict { .. })
        ));

        // Now delete lock for ref2
        ls.delete_locks(&[ref2]).unwrap();
        // Confirm the deletion succeeded
        assert_eq!(
            ls.get_lock(ref2),
            Err(SuiError::ObjectNotFound {
                object_id: ref2.0,
                version: None
            })
        );

        // Initialize the object's entry to another version
        let new_ref2 = (ref2.0, ref2.1.increment(), ref2.2);
        ls.initialize_locks(&[new_ref2], false /* is_force_reset */)
            .unwrap();

        // Now we get ObjectVersionUnavailableForConsumption
        let lock_info = ls.get_lock(ref2).unwrap();
        assert_eq!(lock_info.current_object_record(), new_ref2);
        assert!(!lock_info.is_initialized_or_locked_at_given_version());
        assert!(!lock_info.is_locked_at_given_version());
        assert_eq!(lock_info.tx_locks_given_version(), None);
        assert!(matches!(
            ls.acquire_locks(0, &[ref2, ref3], tx2),
            Err(SuiError::ObjectVersionUnavailableForConsumption {
                provided_obj_ref,
                current_version,
            })
            if provided_obj_ref == ref2 && current_version == new_ref2.1
        ));
        assert_eq!(
            ls.locks_exist(&[ref2, ref3]),
            Err(SuiError::ObjectVersionUnavailableForConsumption {
                provided_obj_ref: ref2,
                current_version: new_ref2.1
            })
        );
    }

    #[tokio::test]
    async fn test_lockdb_remove_multiple() {
        let ls = init_lockservice_db();

        let ref1: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());
        let ref2: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());

        let tx1 = TransactionDigest::random();

        // Initialize 2 locks
        ls.initialize_locks(&[ref1, ref2], false /* is_force_reset */)
            .unwrap();

        // Should be able to acquire lock if all objects initialized
        ls.acquire_locks(0, &[ref1, ref2], tx1).unwrap();
        let lock_info = ls.get_lock(ref2).unwrap();
        assert_eq!(lock_info.current_object_record(), ref2);
        assert!(lock_info.is_initialized_or_locked_at_given_version());
        assert!(lock_info.is_locked_at_given_version());
        assert_eq!(
            lock_info.tx_locks_given_version(),
            Some(&LockDetails {
                epoch: 0,
                tx_digest: tx1
            })
        );

        // Cannot initialize them again since they are locked already
        assert!(matches!(
            ls.initialize_locks(&[ref1, ref2], false /* is_force_reset */),
            Err(SuiError::ObjectLockAlreadyInitialized { .. })
        ));

        // Now remove the locks
        ls.delete_locks(&[ref1, ref2]).unwrap();
        assert_eq!(
            ls.get_lock(ref2),
            Err(SuiError::ObjectNotFound {
                object_id: ref2.0,
                version: None
            })
        );

        // Now initialization should succeed
        ls.initialize_locks(&[ref1, ref2], false /* is_force_reset */)
            .unwrap();
    }

    #[tokio::test]
    async fn test_lockservice_conc_acquire_init() {
        telemetry_subscribers::init_for_testing();
        let ls = init_lockservice();

        let ref1: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());
        let ref2: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());
        let txdigests: Vec<TransactionDigest> =
            (0..10).map(|_n| TransactionDigest::random()).collect();

        // Should be able to concurrently initialize locks for same objects, all should succeed
        let futures = (0..10).map(|_n| {
            let ls = ls.clone();
            tokio::spawn(async move {
                ls.initialize_locks(&[ref1, ref2], false /* is_force_reset */)
                    .await
            })
        });
        let results = join_all(futures).await;
        assert!(results.iter().all(|res| res.is_ok()));

        let lock_info = ls.get_lock(ref1).await.unwrap();
        assert_eq!(lock_info.current_object_record(), ref1);
        assert!(lock_info.is_initialized_or_locked_at_given_version());
        assert!(!lock_info.is_locked_at_given_version());
        assert!(lock_info.tx_locks_given_version().is_none());

        assert_eq!(ls.locks_exist(vec![ref1, ref2]).await, Ok(()));

        // only one party should be able to successfully acquire the lock.  Use diff tx for each one
        let futures = txdigests.iter().map(|tx| {
            let ls = ls.clone();
            let tx = *tx;
            tokio::spawn(async move { ls.acquire_locks(0, vec![ref1, ref2], tx).await })
        });
        let results = join_all(futures).await;
        let inner_res: Vec<_> = results.into_iter().map(|r| r.unwrap()).collect();
        let num_oks = inner_res.iter().filter(|r| r.is_ok()).count();
        assert_eq!(num_oks, 1);

        assert_eq!(ls.locks_exist(vec![ref1, ref2]).await, Ok(()));

        // All other results should be ConflictingTransaction
        assert!(inner_res
            .iter()
            .filter(|r| r.is_err())
            .all(|r| matches!(r, Err(SuiError::ObjectLockConflict { .. }))));
    }

    #[tokio::test]
    async fn test_lockdb_relock_at_new_epoch() {
        let ls = init_lockservice_db();

        let ref1: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());
        let ref2: ObjectRef = (ObjectID::random(), 1.into(), ObjectDigest::random());

        let tx1 = TransactionDigest::random();
        let tx2 = TransactionDigest::random();

        // Initialize 2 locks
        ls.initialize_locks(&[ref1, ref2], false /* is_force_reset */)
            .unwrap();

        let lock_info = ls.get_lock(ref2).unwrap();
        assert_eq!(lock_info.current_object_record(), ref2);
        assert!(lock_info.is_initialized_or_locked_at_given_version());
        assert!(!lock_info.is_locked_at_given_version());
        assert!(lock_info.tx_locks_given_version().is_none());

        assert_eq!(ls.locks_exist(&[ref1, ref2]), Ok(()));

        // Should be able to acquire lock if all objects initialized
        ls.acquire_locks(0, &[ref1, ref2], tx1).unwrap();

        // Try to acquire lock for the same object with a different transaction should fail.
        assert!(ls.acquire_locks(0, &[ref1], tx2).is_err());
        // The object is still locked at the same transaction.
        let lock_info = ls.get_lock(ref1).unwrap();
        assert_eq!(lock_info.current_object_record(), ref1);
        assert!(lock_info.is_initialized_or_locked_at_given_version());
        assert!(lock_info.is_locked_at_given_version());
        assert_eq!(lock_info.tx_locks_given_version().unwrap().tx_digest, tx1);

        // We should be able to relock the same object with a different transaction from a new epoch.
        ls.acquire_locks(1, &[ref1], tx2).unwrap();
        // The object is now locked at transaction tx2.
        let lock_info = ls.get_lock(ref1).unwrap();
        assert_eq!(lock_info.current_object_record(), ref1);
        assert!(lock_info.is_initialized_or_locked_at_given_version());
        assert!(lock_info.is_locked_at_given_version());
        assert_eq!(
            lock_info.tx_locks_given_version().unwrap(),
            &LockDetails {
                epoch: 1,
                tx_digest: tx2
            }
        );

        // Since ref1 is now locked by tx2, we cannot relock it at the same epoch.
        assert!(ls.acquire_locks(1, &[ref1, ref2], tx1).is_err());

        // ref1 is already locked by tx2, and hence this is a nop. ref2 is still locked by tx1 from
        // epoch 0, which will be overridden here.
        ls.acquire_locks(1, &[ref1, ref2], tx2).unwrap();

        let lock_info = ls.get_lock(ref1).unwrap();
        assert_eq!(lock_info.current_object_record(), ref1);
        assert!(lock_info.is_initialized_or_locked_at_given_version());
        assert!(lock_info.is_locked_at_given_version());
        assert_eq!(
            lock_info.tx_locks_given_version().unwrap(),
            &LockDetails {
                epoch: 1,
                tx_digest: tx2
            }
        );

        let lock_info = ls.get_lock(ref2).unwrap();
        assert_eq!(lock_info.current_object_record(), ref2);
        assert!(lock_info.is_initialized_or_locked_at_given_version());
        assert!(lock_info.is_locked_at_given_version());
        assert_eq!(
            lock_info.tx_locks_given_version().unwrap(),
            &LockDetails {
                epoch: 1,
                tx_digest: tx2
            }
        );
    }
}
