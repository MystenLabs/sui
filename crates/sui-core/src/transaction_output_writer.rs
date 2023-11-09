// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::in_mem_execution_cache::ExecutionCacheWrite;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use sui_types::base_types::EpochId;
use sui_types::base_types::ObjectRef;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::error::SuiResult;
use sui_types::inner_temporary_store::{InnerTemporaryStore, WrittenObjects};
use sui_types::storage::{MarkerValue, ObjectKey};
use sui_types::transaction::{TransactionDataAPI, VerifiedTransaction};

/// TransactionOutputs
pub struct TransactionOutputs {
    pub transaction: Arc<VerifiedTransaction>,
    pub effects: TransactionEffects,
    pub events: TransactionEvents,

    pub markers: Vec<(ObjectKey, MarkerValue)>,
    pub wrapped: Vec<ObjectKey>,
    pub deleted: Vec<ObjectKey>,
    pub locks_to_delete: Vec<ObjectRef>,
    pub written: WrittenObjects,
}

pub(crate) struct TransactionOutputWriter {
    cache: Arc<dyn ExecutionCacheWrite>,
}

impl TransactionOutputWriter {
    pub fn new(cache: Arc<dyn ExecutionCacheWrite>) -> Self {
        Self { cache }
    }

    /// Write the output of a transaction.
    ///
    /// Because of the child object consistency rule (readers that observe parents must observe all
    /// children of that parent, up to the parent's version bound), implementations of this method
    /// must not write any top-level (address-owned or shared) objects before they have written all
    /// of the object-owned objects in the `objects` list.
    ///
    /// In the future, we may modify this method to expose finer-grained information about
    /// parent/child relationships. (This may be especially necessary for distributed object
    /// storage, but is unlikely to be an issue before we tackle that problem).
    ///
    /// This function should normally return synchronously. However, it is async in order to
    /// allow the cache to implement backpressure. If writes cannot be flushed to durable storage
    /// as quickly as they are arriving via this method, then we may have to wait for the write to
    /// complete.
    ///
    /// This function may evict the mutable input objects (and successfully received objects) of
    /// transaction from the cache, since they cannot be read by any other transaction.
    ///
    /// Any write performed by this method immediately notifies any waiter that has previously
    /// called notify_read_objects_for_execution or notify_read_objects_for_signing for the object
    /// in question.
    pub async fn write_transaction_outputs(
        &self,
        epoch_id: EpochId,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        inner_temporary_store: InnerTemporaryStore,
    ) -> SuiResult {
        self.cache.update_state(
            epoch_id,
            Self::build_transaction_outputs(transaction, effects, inner_temporary_store),
        )
    }
}

impl TransactionOutputWriter {
    // Convert InnerTemporaryStore + Effects into the exact set of updates
    fn build_transaction_outputs(
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        inner_temporary_store: InnerTemporaryStore,
    ) -> TransactionOutputs {
        let InnerTemporaryStore {
            input_objects,
            mutable_inputs,
            written,
            events,
            max_binary_format_version: _,
            loaded_runtime_objects: _,
            no_extraneous_module_bytes: _,
            runtime_packages_loaded_from_db: _,
            lamport_version,
        } = inner_temporary_store;

        let tx_digest = *transaction.digest();

        let deleted: HashMap<_, _> = effects.all_tombstones().into_iter().collect();

        // Get the actual set of objects that have been received -- any received
        // object will show up in the modified-at set.
        let modified_at: HashSet<_> = effects.modified_at_versions().into_iter().collect();
        let possible_to_receive = transaction.transaction_data().receiving_objects();
        let received_objects = possible_to_receive
            .iter()
            .cloned()
            .filter(|obj_ref| modified_at.contains(&(obj_ref.0, obj_ref.1)));

        // We record any received or deleted objects since they could be pruned, and smear shared
        // object deletions in the marker table. For deleted entries in the marker table we need to
        // make sure we don't accidentally overwrite entries.
        let markers: Vec<_> = {
            let received = received_objects
                .clone()
                .map(|objref| (ObjectKey::from(objref), MarkerValue::Received));

            let deleted = deleted.into_iter().map(|(object_id, version)| {
                let object_key = ObjectKey(object_id, version);
                if input_objects
                    .get(&object_id)
                    .is_some_and(|object| object.is_shared())
                {
                    (object_key, MarkerValue::SharedDeleted(tx_digest))
                } else {
                    (object_key, MarkerValue::OwnedDeleted)
                }
            });

            // We "smear" shared deleted objects in the marker table to allow for proper sequencing
            // of transactions that are submitted after the deletion of the shared object.
            // NB: that we do _not_ smear shared objects that were taken immutably in the
            // transaction.
            let smeared_objects = effects.deleted_mutably_accessed_shared_objects();
            let shared_smears = smeared_objects.into_iter().map(move |object_id| {
                (
                    ObjectKey(object_id, lamport_version),
                    MarkerValue::SharedDeleted(tx_digest),
                )
            });

            received.chain(deleted).chain(shared_smears).collect()
        };

        let locks_to_delete: Vec<_> = mutable_inputs
            .into_iter()
            .filter_map(|(id, ((version, digest), owner))| {
                owner.is_address_owned().then_some((id, version, digest))
            })
            .chain(received_objects)
            .collect();

        let deleted = effects
            .deleted()
            .into_iter()
            .chain(effects.unwrapped_then_deleted())
            .map(ObjectKey::from)
            .collect();

        let wrapped = effects.wrapped().into_iter().map(ObjectKey::from).collect();

        TransactionOutputs {
            transaction: Arc::new(transaction),
            effects,
            events,
            markers,
            wrapped,
            deleted,
            locks_to_delete,
            written,
        }
    }
}
