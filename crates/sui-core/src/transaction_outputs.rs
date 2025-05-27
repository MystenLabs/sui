// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use sui_types::base_types::{FullObjectID, ObjectRef};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::inner_temporary_store::{InnerTemporaryStore, WrittenObjects};
use sui_types::storage::{FullObjectKey, InputKey, MarkerValue, ObjectKey};
use sui_types::transaction::{TransactionDataAPI, VerifiedTransaction};

/// TransactionOutputs
pub struct TransactionOutputs {
    pub transaction: Arc<VerifiedTransaction>,
    pub effects: TransactionEffects,
    pub events: TransactionEvents,

    pub markers: Vec<(FullObjectKey, MarkerValue)>,
    pub wrapped: Vec<ObjectKey>,
    pub deleted: Vec<ObjectKey>,
    pub locks_to_delete: Vec<ObjectRef>,
    pub new_locks_to_init: Vec<ObjectRef>,
    pub written: WrittenObjects,

    // Temporarily needed to notify TxManager about the availability of objects.
    // TODO: Remove this once we ship the new ExecutionScheduler.
    pub output_keys: Vec<InputKey>,
}

impl TransactionOutputs {
    // Convert InnerTemporaryStore + Effects into the exact set of updates to the store
    pub fn build_transaction_outputs(
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        inner_temporary_store: InnerTemporaryStore,
    ) -> TransactionOutputs {
        let output_keys = inner_temporary_store.get_output_keys(&effects);

        let InnerTemporaryStore {
            input_objects,
            stream_ended_consensus_objects,
            mutable_inputs,
            written,
            events,
            loaded_runtime_objects: _,
            binary_config: _,
            runtime_packages_loaded_from_db: _,
            lamport_version,
        } = inner_temporary_store;

        let tx_digest = *transaction.digest();

        let tombstones: HashMap<_, _> = effects.all_tombstones().into_iter().collect();

        // Get the actual set of objects that have been received -- any received
        // object will show up in the modified-at set.
        let modified_at: HashSet<_> = effects.modified_at_versions().into_iter().collect();
        let possible_to_receive = transaction.transaction_data().receiving_objects();
        let received_objects = possible_to_receive
            .into_iter()
            .filter(|obj_ref| modified_at.contains(&(obj_ref.0, obj_ref.1)));

        // We record any received or deleted objects since they could be pruned, and smear object
        // removals from consensus in the marker table. For deleted entries in the marker table we
        // need to make sure we don't accidentally overwrite entries.
        let markers: Vec<_> = {
            let received = received_objects.clone().map(|objref| {
                (
                    // TODO: Add support for receiving consensus objects. For now this assumes fastpath.
                    FullObjectKey::new(FullObjectID::new(objref.0, None), objref.1),
                    MarkerValue::Received,
                )
            });

            let tombstones = tombstones.into_iter().map(|(object_id, version)| {
                let consensus_key = input_objects
                    .get(&object_id)
                    .filter(|o| o.is_consensus())
                    .map(|o| FullObjectKey::new(o.full_id(), version));
                if let Some(consensus_key) = consensus_key {
                    (consensus_key, MarkerValue::ConsensusStreamEnded(tx_digest))
                } else {
                    (
                        FullObjectKey::new(FullObjectID::new(object_id, None), version),
                        MarkerValue::FastpathStreamEnded,
                    )
                }
            });

            let transferred_to_consensus =
                effects
                    .transferred_to_consensus()
                    .into_iter()
                    .map(|(object_id, version, _)| {
                        // Note: it's a bit of a misnomer to mark an object as `FastpathStreamEnded`
                        // when it could have been transferred to consensus from `ObjectOwner`, as
                        // its root owner may not have been a fastpath object. However, whether or
                        // not it was technically in the fastpath at the version the marker is
                        // written, it cetainly is not in the fastpath *anymore*. This is needed
                        // to produce the required behavior in `ObjectCacheRead::multi_input_objects_available`
                        // when checking whether receiving objects are available.
                        (
                            FullObjectKey::new(FullObjectID::new(object_id, None), version),
                            MarkerValue::FastpathStreamEnded,
                        )
                    });

            let transferred_from_consensus =
                effects
                    .transferred_from_consensus()
                    .into_iter()
                    .map(|(object_id, version, _)| {
                        let object = input_objects
                            .get(&object_id)
                            .expect("object transferred from consensus must be in input_objects");
                        (
                            FullObjectKey::new(object.full_id(), version),
                            MarkerValue::ConsensusStreamEnded(tx_digest),
                        )
                    });

            // We "smear" removed consensus objects in the marker table to allow for proper
            // sequencing of transactions that are submitted after the consensus stream ends.
            // This means writing duplicate copies of the `ConsensusStreamEnded` marker for
            // every output version that was scheduled to be created.
            // NB: that we do _not_ smear objects that were taken immutably in the transaction
            // (because these are not assigned output versions).
            let smeared_objects = effects.stream_ended_mutably_accessed_consensus_objects();
            let consensus_smears = smeared_objects.into_iter().map(|object_id| {
                let id = input_objects
                    .get(&object_id)
                    .map(|obj| obj.full_id())
                    .unwrap_or_else(|| {
                        let start_version = stream_ended_consensus_objects.get(&object_id)
                            .expect("stream-ended object must be in either input_objects or stream_ended_consensus_objects");
                        FullObjectID::new(object_id, Some(*start_version))
                    });
                (
                    FullObjectKey::new(id, lamport_version),
                    MarkerValue::ConsensusStreamEnded(tx_digest),
                )
            });

            received
                .chain(tombstones)
                .chain(transferred_to_consensus)
                .chain(transferred_from_consensus)
                .chain(consensus_smears)
                .collect()
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
            output_keys,
        }
    }
}
