// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod coin_balance_buckets;
pub(crate) mod cp_sequence_numbers;
pub(crate) mod ev_emit_mod;
pub(crate) mod ev_struct_inst;
pub(crate) mod kv_checkpoints;
pub(crate) mod kv_epoch_ends;
pub(crate) mod kv_epoch_starts;
pub(crate) mod kv_feature_flags;
pub(crate) mod kv_objects;
pub(crate) mod kv_protocol_configs;
pub(crate) mod kv_transactions;
pub(crate) mod obj_info;
pub(crate) mod obj_info_temp;
pub(crate) mod obj_versions;
pub(crate) mod sum_displays;
pub(crate) mod sum_packages;
pub(crate) mod tx_affected_addresses;
pub(crate) mod tx_affected_objects;
pub(crate) mod tx_balance_changes;
pub(crate) mod tx_calls;
pub(crate) mod tx_digests;
pub(crate) mod tx_kinds;

use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

use sui_indexer_alt_framework::types::{
    base_types::{ObjectID, SequenceNumber},
    effects::TransactionEffectsAPI,
    full_checkpoint_content::CheckpointData,
    object::Object,
};

/// Returns the first appearance of all objects that were used as inputs to the transactions in the
/// checkpoint. These are objects that existed prior to the checkpoint, and excludes objects that
/// were created or unwrapped within the checkpoint.
pub(crate) fn checkpoint_input_objects(
    checkpoint: &Arc<CheckpointData>,
) -> BTreeMap<ObjectID, &Object> {
    let mut output_objects_seen = HashSet::new();
    let mut checkpoint_input_objects = BTreeMap::new();
    for tx in checkpoint.transactions.iter() {
        let input_objects_map: BTreeMap<(ObjectID, SequenceNumber), &Object> = tx
            .input_objects
            .iter()
            .map(|obj| ((obj.id(), obj.version()), obj))
            .collect();

        for change in tx.effects.object_changes() {
            let id = change.id;
            // Handle input objects - only track first appearance
            if let Some(version) = change.input_version {
                // If the object appears in `checkpoint_object_changes`, it was an input object that
                // was previously modified or unwrapped. In both cases, we'd ignore this newer
                // entry
                if output_objects_seen.contains(&id) || checkpoint_input_objects.contains_key(&id) {
                    continue;
                }

                let input_obj = input_objects_map
                                .get(&(id, version))
                                .copied()
                                .unwrap_or_else(|| panic!(
                                    "Object {id} at version {version} referenced in tx.effects.object_changes() not found in tx.input_objects"
                                ));
                checkpoint_input_objects.insert(id, input_obj);
            }
        }

        for obj in tx.output_objects.iter() {
            output_objects_seen.insert(obj.id());
        }
    }
    checkpoint_input_objects
}
