// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Indexer pipelines that populate the `sui-rpc-store` schema
//! from observed [`Checkpoint`]s.
//!
//! Each submodule implements the `Processor` + `sequential::Handler`
//! pair the `sui-indexer-alt-framework` drives: `process` turns a
//! checkpoint into a `Vec<Value>` (with the heavy lifting done in
//! the processor-pool, off the commit hot path), `batch` folds
//! many values into a single `Batch`, and `commit` stages the
//! batch's writes against a [`Connection`] from
//! [`sui_consistent_store::Store`].
//!
//! Every pipeline targets the same backing [`RpcStoreSchema`].

pub mod balance;
pub mod checkpoint_contents;
pub mod checkpoint_seq_by_digest;
pub mod checkpoint_summary;
pub mod effects;
pub mod epochs;
pub mod event_bitmap;
pub mod events;
pub mod live_objects;
pub mod object_by_owner;
pub mod object_by_type;
pub mod objects;
pub mod package_versions;
pub mod transaction_bitmap;
pub mod transactions;
pub mod tx_metadata_by_seq;
pub mod tx_seq_by_digest;

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::collections::btree_map::Entry;

use anyhow::Context as _;
use sui_types::base_types::ObjectID;
use sui_types::digests::ObjectDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Object;

use crate::RpcStoreSchema;

/// The schema parameter the framework's `Store` / pipelines bind
/// to.
pub type Schema = RpcStoreSchema;

/// The store type pipelines commit through.
pub type Store = sui_consistent_store::Store<Schema>;

/// The sequence number of the first transaction in `checkpoint`.
///
/// `network_total_transactions` is the cumulative network-wide tx
/// count *after* this checkpoint executes, so subtracting the
/// number of transactions the checkpoint contains gives the
/// `tx_seq` of its first entry.
pub fn first_tx_seq(checkpoint: &Checkpoint) -> u64 {
    checkpoint.summary.network_total_transactions - checkpoint.transactions.len() as u64
}

/// The `tx_seq` of the transaction at index `i` within
/// `checkpoint`.
pub fn tx_seq_at(checkpoint: &Checkpoint, i: usize) -> u64 {
    first_tx_seq(checkpoint) + i as u64
}

/// First-seen input version of every object that existed before
/// the checkpoint and was used as an input to some transaction in
/// it. Mirrors the helper of the same name in
/// `sui-indexer-alt-consistent-store::handlers`.
///
/// Objects created or unwrapped within the checkpoint are
/// excluded. Used by the diff-based indexes
/// ([`object_by_owner`](crate::indexer::object_by_owner) etc.) to
/// remove the rows that the *prior* state contributed before
/// re-inserting the rows that the *posterior* state contributes.
pub fn checkpoint_input_objects(
    checkpoint: &Checkpoint,
) -> anyhow::Result<BTreeMap<ObjectID, (&Object, ObjectDigest)>> {
    let mut from_this_checkpoint = HashSet::new();
    let mut input_objects = BTreeMap::new();
    for tx in &checkpoint.transactions {
        let input_objects_map: BTreeMap<_, _> = tx
            .input_objects(&checkpoint.object_set)
            .map(|obj| ((obj.id(), obj.version()), obj))
            .collect();

        for change in tx.effects.object_changes() {
            let id = change.id;

            let Some(version) = change.input_version else {
                continue;
            };

            if from_this_checkpoint.contains(&id) {
                continue;
            }

            let Entry::Vacant(entry) = input_objects.entry(id) else {
                continue;
            };

            let input_object = *input_objects_map
                .get(&(id, version))
                .with_context(|| format!("{id} at {version} in effects, not in input_objects"))?;

            // Input digests are only populated in Effects V2. For Effects V1, we need to
            // compute the digest from the input object's contents.
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

/// Last-seen output version of every object that was created or
/// modified by some transaction in the checkpoint and is still
/// live at the end. Mirrors the helper of the same name in
/// `sui-indexer-alt-consistent-store::handlers`.
///
/// Used to populate the latest-version views
/// ([`live_objects`](crate::indexer::live_objects)) and the
/// diff-based indexes once the prior state has been retracted.
pub fn checkpoint_output_objects(
    checkpoint: &Checkpoint,
) -> anyhow::Result<BTreeMap<ObjectID, (&Object, ObjectDigest)>> {
    let mut output_objects = BTreeMap::new();
    for tx in &checkpoint.transactions {
        let output_objects_map: BTreeMap<_, _> = tx
            .output_objects(&checkpoint.object_set)
            .map(|obj| ((obj.id(), obj.version()), obj))
            .collect();

        for change in tx.effects.object_changes() {
            let id = change.id;

            // Clear the previous entry, in case it was created within this checkpoint.
            output_objects.remove(&id);

            let (Some(version), Some(digest)) = (change.output_version, change.output_digest)
            else {
                continue;
            };

            let output_object = *output_objects_map
                .get(&(id, version))
                .with_context(|| format!("{id} at {version} in effects, not in output_objects"))?;

            output_objects.insert(id, (output_object, digest));
        }
    }
    Ok(output_objects)
}
