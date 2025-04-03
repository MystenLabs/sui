// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashSet};

use crate::base_types::{ObjectID, ObjectRef, SequenceNumber};
use crate::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use crate::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointContents};
use crate::object::Object;
use crate::storage::BackingPackageStore;
use crate::transaction::Transaction;
use serde::{Deserialize, Serialize};
use tap::Pipe;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointData {
    pub checkpoint_summary: CertifiedCheckpointSummary,
    pub checkpoint_contents: CheckpointContents,
    pub transactions: Vec<CheckpointTransaction>,
}

pub struct CheckpointObject<'input, 'output> {
    pub object_id: ObjectID,
    pub input_state: CheckpointObjectInputState<'input>,
    pub output_state: CheckpointObjectOutputState<'output>,
}

pub enum CheckpointObjectInputState<'a> {
    /// The object existed before the current checkpoint, and has object contents.
    BeforeCheckpoint(&'a Object),
    /// The object was created or unwrapped at some transaction in the checkpoint.
    CreatedInCheckpoint,
}

pub enum CheckpointObjectOutputState<'a> {
    /// The object was created, mutated, or unwrapped at the end of the checkpoint.
    Mutated(&'a Object),
    /// The object was wrapped or deleted at the end of the checkpoint.
    WrappedOrDeleted,
}

impl CheckpointData {
    // returns the latest versions of the output objects that still exist at the end of the checkpoint
    pub fn latest_live_output_objects(&self) -> Vec<&Object> {
        let mut latest_live_objects = BTreeMap::new();
        for tx in self.transactions.iter() {
            for obj in tx.output_objects.iter() {
                latest_live_objects.insert(obj.id(), obj);
            }
            for obj_ref in tx.removed_object_refs_post_version() {
                latest_live_objects.remove(&(obj_ref.0));
            }
        }
        latest_live_objects.into_values().collect()
    }

    // returns the object refs that are eventually deleted or wrapped in the current checkpoint
    pub fn eventually_removed_object_refs_post_version(&self) -> Vec<ObjectRef> {
        let mut eventually_removed_object_refs = BTreeMap::new();
        for tx in self.transactions.iter() {
            for obj_ref in tx.removed_object_refs_post_version() {
                eventually_removed_object_refs.insert(obj_ref.0, obj_ref);
            }
            for obj in tx.output_objects.iter() {
                eventually_removed_object_refs.remove(&(obj.id()));
            }
        }
        eventually_removed_object_refs.into_values().collect()
    }

    /// Returns all objects that are used as input to the transactions in the checkpoint,
    /// and already exist prior to the checkpoint.
    pub fn checkpoint_input_objects(&self) -> BTreeMap<ObjectID, &Object> {
        let mut output_objects_seen = HashSet::new();
        let mut checkpoint_input_objects = BTreeMap::new();
        for tx in self.transactions.iter() {
            // Construct maps of input and output objects for efficient lookup
            let input_objects_map: BTreeMap<(ObjectID, SequenceNumber), &Object> = tx
                .input_objects
                .iter()
                .map(|obj| ((obj.id(), obj.version()), obj))
                .collect();

            for change in tx.effects.object_changes() {
                // Handle input objects - only track first appearance
                if let Some((id, version, _)) = change.input_ref() {
                    // If the object appears in `checkpoint_object_changes`, it was an input object that
                    // was previously modified or unwrapped. In both cases, we'd ignore this newer
                    // entry
                    if output_objects_seen.contains(&id)
                        || checkpoint_input_objects.contains_key(&id)
                    {
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

    pub fn all_objects(&self) -> Vec<&Object> {
        self.transactions
            .iter()
            .flat_map(|tx| &tx.input_objects)
            .chain(self.transactions.iter().flat_map(|tx| &tx.output_objects))
            .collect()
    }

    /// For a checkpoint, aggregate the object changes of its transactions and returns the initial
    /// and final state of each object at the end of the checkpoint as if a single transaction was
    /// executed.
    ///
    /// Uses `object_changes()` from each transaction's effects as the source of truth to:
    /// 1. Track input objects at their first appearance, ignoring unwrapped objects
    /// 2. Track all changes (creations, modifications, wraps, deletes) with their final state at
    ///    checkpoint end. Note that later changes will overwrite existing entries. If an object has
    ///    an output `ObjectRef`, it must have a corresponding entry in `tx.output_objects`.
    pub fn checkpoint_objects(&self) -> Vec<CheckpointObject<'_, '_>> {
        // Tracks only the first appearance of objects named as an input to some transaction in the
        // checkpoint
        let mut checkpoint_input_objects = BTreeMap::new();
        // Tracks all objects and its final state in the checkpoint
        let mut checkpoint_object_changes = BTreeMap::new();

        for tx in self.transactions.iter() {
            // Construct maps of input and output objects for efficient lookup
            let input_objects_map: BTreeMap<(ObjectID, SequenceNumber), &Object> = tx
                .input_objects
                .iter()
                .map(|obj| ((obj.id(), obj.version()), obj))
                .collect();
            let output_objects_map: BTreeMap<ObjectID, &Object> = tx
                .output_objects
                .iter()
                .map(|obj| (obj.id(), obj))
                .collect();

            for change in tx.effects.object_changes() {
                let obj_id = change.id;
                // Handle input objects - only track first appearance
                if let Some((id, version, _)) = change.input_ref() {
                    // If the object appears in `checkpoint_object_changes`, it was an input object that
                    // was previously modified or unwrapped. In both cases, we'd ignore this newer
                    // entry
                    if !checkpoint_object_changes.contains_key(&id)
                        && !checkpoint_input_objects.contains_key(&id)
                    {
                        let input_obj = input_objects_map
                            .get(&(id, version))
                            .copied()
                            .unwrap_or_else(|| panic!(
                                "Object {id} at version {version} referenced in tx.effects.object_changes() not found in tx.input_objects"
                            ));
                        checkpoint_input_objects.insert(id, input_obj);
                    }
                }

                let output_obj = if change.output_ref().is_none() {
                    None
                } else {
                    Some(output_objects_map
                        .get(&obj_id)
                        .copied()
                        .unwrap_or_else(|| panic!(
                            "Output object {obj_id} referenced in tx.effects.object_changes() not found in tx.output_objects. Data inconsistency in CheckpointData's CheckpointTransaction"
                        )))
                };
                checkpoint_object_changes.insert(obj_id, (change, output_obj));
            }
        }

        checkpoint_object_changes
            .into_iter()
            .map(|(id, (_, output_obj))| {
                CheckpointObject {
                    object_id: id,
                    // If in checkpoint_input_objects, it existed before checkpoint
                    // Otherwise it was created/unwrapped during checkpoint
                    input_state: match checkpoint_input_objects.get(&id) {
                        Some(obj) => CheckpointObjectInputState::BeforeCheckpoint(obj),
                        None => CheckpointObjectInputState::CreatedInCheckpoint,
                    },
                    // Final state from the change
                    output_state: match output_obj {
                        Some(obj) => CheckpointObjectOutputState::Mutated(obj),
                        None => CheckpointObjectOutputState::WrappedOrDeleted,
                    },
                }
            })
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointTransaction {
    /// The input Transaction
    pub transaction: Transaction,
    /// The effects produced by executing this transaction
    pub effects: TransactionEffects,
    /// The events, if any, emitted by this transactions during execution
    pub events: Option<TransactionEvents>,
    /// The state of all inputs to this transaction as they were prior to execution.
    pub input_objects: Vec<Object>,
    /// The state of all output objects created or mutated or unwrapped by this transaction.
    pub output_objects: Vec<Object>,
}

impl CheckpointTransaction {
    // provide an iterator over all deleted or wrapped objects in this transaction
    pub fn removed_objects_pre_version(&self) -> impl Iterator<Item = &Object> {
        // Since each object ID can only show up once in the input_objects, we can just use the
        // ids of deleted and wrapped objects to lookup the object in the input_objects.
        self.effects
            .all_removed_objects()
            .into_iter() // Use id and version to lookup in input Objects
            .map(|((id, _, _), _)| {
                self.input_objects
                    .iter()
                    .find(|o| o.id() == id)
                    .expect("all removed objects should show up in input objects")
            })
    }

    pub fn removed_object_refs_post_version(&self) -> impl Iterator<Item = ObjectRef> {
        let deleted = self.effects.deleted().into_iter();
        let wrapped = self.effects.wrapped().into_iter();
        let unwrapped_then_deleted = self.effects.unwrapped_then_deleted().into_iter();
        deleted.chain(wrapped).chain(unwrapped_then_deleted)
    }

    pub fn changed_objects(&self) -> impl Iterator<Item = (&Object, Option<&Object>)> {
        self.effects
            .all_changed_objects()
            .into_iter()
            .map(|((id, _, _), ..)| {
                let object = self
                    .output_objects
                    .iter()
                    .find(|o| o.id() == id)
                    .expect("changed objects should show up in output objects");

                let old_object = self.input_objects.iter().find(|o| o.id() == id);

                (object, old_object)
            })
    }

    pub fn created_objects(&self) -> impl Iterator<Item = &Object> {
        // Iterator over (ObjectId, version) for created objects
        self.effects
            .created()
            .into_iter()
            // Lookup Objects in output Objects as well as old versions for mutated objects
            .map(|((id, version, _), _)| {
                self.output_objects
                    .iter()
                    .find(|o| o.id() == id && o.version() == version)
                    .expect("created objects should show up in output objects")
            })
    }
}

impl BackingPackageStore for CheckpointData {
    fn get_package_object(
        &self,
        package_id: &crate::base_types::ObjectID,
    ) -> crate::error::SuiResult<Option<crate::storage::PackageObject>> {
        self.transactions
            .iter()
            .flat_map(|transaction| transaction.output_objects.iter())
            .find(|object| object.is_package() && &object.id() == package_id)
            .cloned()
            .map(crate::storage::PackageObject::new)
            .pipe(Ok)
    }
}
