// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{btree_map::Entry, BTreeMap, HashSet},
    sync::Arc,
};

use anyhow::Context;
use sui_indexer_alt_framework::types::{
    base_types::{ObjectID, SequenceNumber},
    effects::TransactionEffectsAPI,
    full_checkpoint_content::CheckpointData,
    object::Object,
};

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

/// Returns the first appearance of all objects that were used as inputs to the transactions in the
/// checkpoint. These are objects that existed prior to the checkpoint, and excludes objects that
/// were created or unwrapped within the checkpoint.
pub(crate) fn checkpoint_input_objects(
    checkpoint: &Arc<CheckpointData>,
) -> anyhow::Result<BTreeMap<ObjectID, &Object>> {
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

            let Some(version) = change.input_version else {
                continue;
            };

            // This object was previously modified, created, or unwrapped in the checkpoint, so
            // this version is not a checkpoint input.
            if output_objects_seen.contains(&id) {
                continue;
            }

            // Make sure this object has not already been recorded as an input.
            let Entry::Vacant(entry) = checkpoint_input_objects.entry(id) else {
                continue;
            };

            let input_obj = input_objects_map
                .get(&(id, version))
                .copied()
                .with_context(|| format!(
                    "Object {id} at version {version} referenced in effects not found in input_objects"
                ))?;

            entry.insert(input_obj);
        }

        for change in tx.effects.object_changes() {
            if change.output_version.is_some() {
                output_objects_seen.insert(change.id);
            }
        }
    }

    Ok(checkpoint_input_objects)
}
