// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::base_types::ObjectRef;
use crate::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use crate::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointContents};
use crate::object::Object;
use crate::storage::error::Error as StorageError;
use crate::storage::{BackingPackageStore, EpochInfo};
use crate::sui_system_state::get_sui_system_state;
use crate::sui_system_state::SuiSystemStateTrait;
use crate::transaction::{Transaction, TransactionDataAPI, TransactionKind};
use serde::{Deserialize, Serialize};
use tap::Pipe;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointData {
    pub checkpoint_summary: CertifiedCheckpointSummary,
    pub checkpoint_contents: CheckpointContents,
    pub transactions: Vec<CheckpointTransaction>,
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

    pub fn all_objects(&self) -> Vec<&Object> {
        self.transactions
            .iter()
            .flat_map(|tx| &tx.input_objects)
            .chain(self.transactions.iter().flat_map(|tx| &tx.output_objects))
            .collect()
    }

    pub fn epoch_info(&self) -> Result<Option<EpochInfo>, StorageError> {
        if self.checkpoint_summary.end_of_epoch_data.is_none()
            && self.checkpoint_summary.sequence_number != 0
        {
            return Ok(None);
        }
        let (start_checkpoint, transaction) = if self.checkpoint_summary.sequence_number == 0 {
            (0, &self.transactions[0])
        } else {
            let Some(transaction) = self.transactions.iter().find(|tx| {
                matches!(
                    tx.transaction.intent_message().value.kind(),
                    TransactionKind::ChangeEpoch(_) | TransactionKind::EndOfEpochTransaction(_)
                )
            }) else {
                return Err(StorageError::custom(format!(
                    "Failed to get end of epoch transaction in checkpoint {} with EndOfEpochData",
                    self.checkpoint_summary.sequence_number,
                )));
            };
            (self.checkpoint_summary.sequence_number + 1, transaction)
        };
        let system_state =
            get_sui_system_state(&transaction.output_objects.as_slice()).map_err(|e| {
                StorageError::custom(format!(
                    "Failed to find system state object output from end of epoch transaction: {e}"
                ))
            })?;
        Ok(Some(EpochInfo {
            epoch: system_state.epoch(),
            protocol_version: Some(system_state.protocol_version()),
            start_timestamp_ms: Some(system_state.epoch_start_timestamp_ms()),
            end_timestamp_ms: None,
            start_checkpoint: Some(start_checkpoint),
            end_checkpoint: None,
            reference_gas_price: Some(system_state.reference_gas_price()),
            system_state: Some(system_state),
        }))
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
