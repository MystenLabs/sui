// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{btree_map::Entry, BTreeMap, HashSet};

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
    checkpoint: &CheckpointData,
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

#[cfg(test)]
mod tests {
    use sui_indexer_alt_framework::types::{
        object::Owner, test_checkpoint_data_builder::TestCheckpointDataBuilder,
    };

    use super::*;

    #[test]
    fn test_checkpoint_input_objects_simple() {
        const OWNED_MODIFIED: u64 = 0;
        const OWNED_CREATED: u64 = 1;
        const OWNED_WRAP: u64 = 2;
        const OWNED_UNWRAP: u64 = 3;
        const OWNED_DELETE: u64 = 4;
        const FROZEN_READ: u64 = 5;
        const SHARED_MODIFIED: u64 = 6;
        const SHARED_READ: u64 = 7;

        let mut builder = TestCheckpointDataBuilder::new(0);

        // Set-up state in an initial checkpoint.
        builder = builder
            .start_transaction(0)
            .create_owned_object(OWNED_MODIFIED)
            .create_owned_object(OWNED_WRAP)
            .create_owned_object(OWNED_UNWRAP)
            .create_owned_object(OWNED_DELETE)
            .create_owned_object(FROZEN_READ)
            .create_shared_object(SHARED_MODIFIED)
            .create_shared_object(SHARED_READ)
            .finish_transaction()
            .start_transaction(0)
            .change_object_owner(FROZEN_READ, Owner::Immutable)
            .wrap_object(OWNED_UNWRAP)
            .finish_transaction();

        let setup = builder.build_checkpoint();

        // Operate on the objects in the checkpoint we're going to test.
        builder = builder
            .start_transaction(0)
            .mutate_owned_object(OWNED_MODIFIED)
            .create_owned_object(OWNED_CREATED)
            .finish_transaction()
            .start_transaction(0)
            .wrap_object(OWNED_WRAP)
            .unwrap_object(OWNED_UNWRAP)
            .finish_transaction()
            .start_transaction(0)
            .delete_object(OWNED_DELETE)
            .read_frozen_object(FROZEN_READ)
            .finish_transaction()
            .start_transaction(0)
            .mutate_shared_object(SHARED_MODIFIED)
            .read_shared_object(SHARED_READ)
            .finish_transaction();

        let checkpoint = builder.build_checkpoint();

        eprintln!("Checkpoint: {checkpoint:#?}");

        let setup_versions: BTreeMap<_, _> = setup
            .transactions
            .iter()
            .flat_map(|tx| {
                tx.effects
                    .object_changes()
                    .into_iter()
                    .filter_map(|c| Some((c.id, c.output_version?.value())))
            })
            .collect();

        let setup_version = |idx: u64| -> Option<u64> {
            setup_versions
                .get(&TestCheckpointDataBuilder::derive_object_id(idx))
                .copied()
        };

        let inputs = checkpoint_input_objects(&checkpoint).unwrap();
        let input_version = |idx: u64| -> Option<u64> {
            inputs
                .get(&TestCheckpointDataBuilder::derive_object_id(idx))
                .map(|obj| obj.version().value())
        };

        assert_eq!(input_version(OWNED_MODIFIED), setup_version(OWNED_MODIFIED));
        assert_eq!(input_version(OWNED_CREATED), None);
        assert_eq!(input_version(OWNED_WRAP), setup_version(OWNED_WRAP));
        assert_eq!(input_version(OWNED_UNWRAP), None);
        assert_eq!(input_version(OWNED_DELETE), setup_version(OWNED_DELETE));
        assert_eq!(
            input_version(SHARED_MODIFIED),
            setup_version(SHARED_MODIFIED)
        );

        // Frozen inputs and read-only shared inputs don't show up in object changes, so they
        // aren't going to be treated as a checkpoint input object.
        //
        // (Frozen inputs only show up in the transaction data and unchanged shared inputs show up
        // in their own effects field for unchanged shared inputs).
        assert_eq!(input_version(FROZEN_READ), None);
        assert_eq!(input_version(SHARED_READ), None);
    }

    #[test]
    fn test_checkpoint_input_repeated_modification() {
        let mut builder = TestCheckpointDataBuilder::new(0);

        // Set-up state in an initial checkpoint.
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();

        let setup = builder.build_checkpoint();

        // Modify the same object multiple times in the checkpoint we're going to test.
        builder = builder
            .start_transaction(0)
            .mutate_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .mutate_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .mutate_owned_object(0)
            .finish_transaction();

        let checkpoint = builder.build_checkpoint();

        let id = TestCheckpointDataBuilder::derive_object_id(0);
        let setup_version = setup.transactions[0].output_objects[0].version();

        let inputs = checkpoint_input_objects(&checkpoint).unwrap();
        let input_version = inputs.get(&id).map(|obj| obj.version());
        assert_eq!(input_version, Some(setup_version));
    }

    #[test]
    fn test_checkpoint_input_objects_wrap_unwrap() {
        let mut builder = TestCheckpointDataBuilder::new(0);

        // Set-up state in an initial checkpoint.
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();

        let setup = builder.build_checkpoint();

        // Repeatedly wrap and unwrap the same object in the checkpoint we're going to test.
        builder = builder
            .start_transaction(0)
            .wrap_object(0)
            .finish_transaction()
            .start_transaction(0)
            .unwrap_object(0)
            .finish_transaction()
            .start_transaction(0)
            .wrap_object(0)
            .finish_transaction()
            .start_transaction(0)
            .unwrap_object(0)
            .finish_transaction();

        let checkpoint = builder.build_checkpoint();

        let id = TestCheckpointDataBuilder::derive_object_id(0);
        let setup_version = setup.transactions[0].output_objects[0].version();

        let inputs = checkpoint_input_objects(&checkpoint).unwrap();
        let input_version = inputs.get(&id).map(|obj| obj.version());
        assert_eq!(input_version, Some(setup_version));
    }
}
