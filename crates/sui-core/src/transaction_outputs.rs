// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use sui_types::base_types::ObjectRef;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
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
    pub new_locks_to_init: Vec<ObjectRef>,
    pub written: WrittenObjects,
}

impl TransactionOutputs {
    // Convert InnerTemporaryStore + Effects into the exact set of updates to the store
    pub fn build_transaction_outputs(
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        inner_temporary_store: InnerTemporaryStore,
    ) -> TransactionOutputs {
        let InnerTemporaryStore {
            input_objects,
            mutable_inputs,
            written,
            events,
            loaded_runtime_objects: _,
            binary_config: _,
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

        let new_locks_to_init: Vec<_> = written
            .values()
            .filter_map(|new_object| {
                if new_object.is_address_owned() {
                    Some(new_object.compute_object_reference())
                } else {
                    None
                }
            })
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
            new_locks_to_init,
            written,
        }
    }
}
