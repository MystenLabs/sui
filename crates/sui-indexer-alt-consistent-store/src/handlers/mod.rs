// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{btree_map::Entry, BTreeMap, HashSet};

use anyhow::Context;
use sui_indexer_alt_framework::types::{
    base_types::ObjectID, digests::ObjectDigest, effects::TransactionEffectsAPI,
    full_checkpoint_content::Checkpoint, object::Object, storage::ObjectKey,
};

pub(crate) mod balances;
pub(crate) mod object_by_owner;
pub(crate) mod object_by_type;

/// Returns the first appearance of all objects that were used as inputs to the transactions in the
/// checkpoint. These are objects that existed prior to the checkpoint, and excludes objects that
/// were created or unwrapped within the checkpoint.
pub(crate) fn checkpoint_input_objects(
    checkpoint: &Checkpoint,
) -> anyhow::Result<BTreeMap<ObjectID, (&Object, ObjectDigest)>> {
    let mut from_this_checkpoint = HashSet::new();
    let mut input_objects = BTreeMap::new();
    for tx in &checkpoint.transactions {
        for change in tx.effects.object_changes() {
            let id = change.id;

            // Get input version - if None, this object was created in this transaction
            let Some(version) = change.input_version else {
                continue;
            };

            // This object was previously modified, created, or unwrapped in the checkpoint, so
            // this version is not a checkpoint input.
            if from_this_checkpoint.contains(&id) {
                continue;
            }

            // Make sure this object has not already been recorded as an input.
            let Entry::Vacant(entry) = input_objects.entry(id) else {
                continue;
            };

            let input_object = checkpoint
                .object_set
                .get(&ObjectKey(id, version))
                .with_context(|| format!("{id} at {version} in effects, not in object_set"))?;

            // Input digests are only populated in Effects V2. For Effects V1, we need to calculate
            // the digest from the input object's contents.
            let digest = change.input_digest.unwrap_or_else(|| input_object.digest());

            entry.insert((input_object, digest));
        }

        for change in tx.effects.object_changes() {
            if change.output_version.is_some() {
                from_this_checkpoint.insert(change.id);
            }
        }
    }

    Ok(input_objects)
}

/// Returns all versions of objects that were output by transactions in the checkpoint, and are
/// still live at the end of the checkpoint.
pub(crate) fn checkpoint_output_objects(
    checkpoint: &Checkpoint,
) -> anyhow::Result<BTreeMap<ObjectID, (&Object, ObjectDigest)>> {
    let mut output_objects = BTreeMap::new();
    for tx in &checkpoint.transactions {
        for change in tx.effects.object_changes() {
            let id = change.id;

            // Clear the previous entry, in case it was created within this checkpoint.
            output_objects.remove(&id);

            let (Some(version), Some(digest)) = (change.output_version, change.output_digest)
            else {
                continue;
            };

            let output_object = checkpoint
                .object_set
                .get(&ObjectKey(id, version))
                .with_context(|| format!("{id} at {version} in effects, not in object_set"))?;

            output_objects.insert(id, (output_object, digest));
        }
    }

    Ok(output_objects)
}
