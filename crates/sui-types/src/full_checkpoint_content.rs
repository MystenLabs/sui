// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::base_types::{ExecutionData, ObjectRef};
use crate::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use crate::messages_checkpoint::{CertifiedCheckpointSummary, CheckpointContents};
use crate::object::Object;
use crate::signature::GenericSignature;
use crate::storage::ObjectKey;
use crate::storage::error::Error as StorageError;
use crate::storage::{BackingPackageStore, EpochInfo};
use crate::sui_system_state::SuiSystemStateTrait;
use crate::sui_system_state::get_sui_system_state;
use crate::transaction::{Transaction, TransactionData, TransactionDataAPI, TransactionKind};
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

    pub fn execution_data(&self) -> ExecutionData {
        ExecutionData {
            transaction: self.transaction.clone(),
            effects: self.effects.clone(),
        }
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

// Never remove these asserts!
// These data structures are meant to be used in-memory, for structures that can be persisted in
// storage you should look at the protobuf versions.
static_assertions::assert_not_impl_any!(Checkpoint: serde::Serialize, serde::de::DeserializeOwned);
static_assertions::assert_not_impl_any!(ExecutedTransaction: serde::Serialize, serde::de::DeserializeOwned);
static_assertions::assert_not_impl_any!(ObjectSet: serde::Serialize, serde::de::DeserializeOwned);

#[derive(Clone, Debug)]
pub struct Checkpoint {
    pub summary: CertifiedCheckpointSummary,
    pub contents: CheckpointContents,
    pub transactions: Vec<ExecutedTransaction>,
    pub object_set: ObjectSet,
}

#[derive(Clone, Debug)]
pub struct ExecutedTransaction {
    /// The input Transaction
    pub transaction: TransactionData,
    pub signatures: Vec<GenericSignature>,
    /// The effects produced by executing this transaction
    pub effects: TransactionEffects,
    /// The events, if any, emitted by this transactions during execution
    pub events: Option<TransactionEvents>,
    pub unchanged_loaded_runtime_objects: Vec<ObjectKey>,
}

#[derive(Default, Clone, Debug)]
pub struct ObjectSet(BTreeMap<ObjectKey, Object>);

impl ObjectSet {
    pub fn get(&self, key: &ObjectKey) -> Option<&Object> {
        self.0.get(key)
    }

    pub fn insert(&mut self, object: Object) {
        self.0
            .insert(ObjectKey(object.id(), object.version()), object);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Object> {
        self.0.values()
    }
}

impl From<Checkpoint> for CheckpointData {
    fn from(value: Checkpoint) -> Self {
        let transactions = value
            .transactions
            .into_iter()
            .map(|tx| {
                let input_objects = tx
                    .effects
                    .modified_at_versions()
                    .into_iter()
                    .filter_map(|(object_id, version)| {
                        value
                            .object_set
                            .get(&ObjectKey(object_id, version))
                            .cloned()
                    })
                    .collect::<Vec<_>>();
                let output_objects = tx
                    .effects
                    .all_changed_objects()
                    .into_iter()
                    .filter_map(|(object_ref, _owner, _kind)| {
                        value.object_set.get(&object_ref.into()).cloned()
                    })
                    .collect::<Vec<_>>();

                CheckpointTransaction {
                    transaction: Transaction::from_generic_sig_data(tx.transaction, tx.signatures),
                    effects: tx.effects,
                    events: tx.events,
                    input_objects,
                    output_objects,
                }
            })
            .collect();
        Self {
            checkpoint_summary: value.summary,
            checkpoint_contents: value.contents,
            transactions,
        }
    }
}

// Lossy conversion
impl From<CheckpointData> for Checkpoint {
    fn from(value: CheckpointData) -> Self {
        let mut object_set = ObjectSet::default();
        let transactions = value
            .transactions
            .into_iter()
            .map(|tx| {
                for o in tx
                    .input_objects
                    .into_iter()
                    .chain(tx.output_objects.into_iter())
                {
                    object_set.insert(o);
                }

                let sender_signed = tx.transaction.into_data().into_inner();

                ExecutedTransaction {
                    transaction: sender_signed.intent_message.value,
                    signatures: sender_signed.tx_signatures,
                    effects: tx.effects,
                    events: tx.events,

                    // lossy
                    unchanged_loaded_runtime_objects: Vec::new(),
                }
            })
            .collect();
        Self {
            summary: value.checkpoint_summary,
            contents: value.checkpoint_contents,
            transactions,
            object_set,
        }
    }
}
