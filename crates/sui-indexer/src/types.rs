// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_json_rpc_types::{
    ObjectChange, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_types::base_types::{ObjectDigest, SequenceNumber};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::AggregateAuthoritySignature;
use sui_types::digests::TransactionDigest;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::effects::TransactionEffects;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointCommitment, CheckpointContents, CheckpointDigest,
    CheckpointSequenceNumber, EndOfEpochData,
};
use sui_types::move_package::MovePackage;
use sui_types::object::{Object, Owner};
use sui_types::sui_serde::SuiStructTag;
use sui_types::transaction::SenderSignedData;

use crate::errors::IndexerError;

pub type IndexerResult<T> = Result<T, IndexerError>;

#[derive(Debug, Default)]
pub struct IndexedCheckpoint {
    // TODO: A lot of fields are now redundant with certified_checkpoint and checkpoint_contents.
    pub sequence_number: u64,
    pub checkpoint_digest: CheckpointDigest,
    pub epoch: u64,
    pub tx_digests: Vec<TransactionDigest>,
    pub network_total_transactions: u64,
    pub previous_checkpoint_digest: Option<CheckpointDigest>,
    pub timestamp_ms: u64,
    pub total_gas_cost: i64, // total gas cost could be negative
    pub computation_cost: u64,
    pub storage_cost: u64,
    pub storage_rebate: u64,
    pub non_refundable_storage_fee: u64,
    pub checkpoint_commitments: Vec<CheckpointCommitment>,
    pub validator_signature: AggregateAuthoritySignature,
    pub successful_tx_num: usize,
    pub end_of_epoch_data: Option<EndOfEpochData>,
    pub end_of_epoch: bool,
    pub min_tx_sequence_number: u64,
    pub max_tx_sequence_number: u64,
    // FIXME: Remove the Default derive and make these fields mandatory.
    pub certified_checkpoint: Option<CertifiedCheckpointSummary>,
    pub checkpoint_contents: Option<CheckpointContents>,
}

impl IndexedCheckpoint {
    pub fn from_sui_checkpoint(
        checkpoint: &CertifiedCheckpointSummary,
        contents: &CheckpointContents,
        successful_tx_num: usize,
    ) -> Self {
        let total_gas_cost = checkpoint.epoch_rolling_gas_cost_summary.computation_cost as i64
            + checkpoint.epoch_rolling_gas_cost_summary.storage_cost as i64
            - checkpoint.epoch_rolling_gas_cost_summary.storage_rebate as i64;
        let tx_digests = contents.iter().map(|t| t.transaction).collect::<Vec<_>>();
        let max_tx_sequence_number = checkpoint.network_total_transactions - 1;
        // NOTE: + 1u64 first to avoid subtraction with overflow
        let min_tx_sequence_number = max_tx_sequence_number + 1u64 - tx_digests.len() as u64;
        let auth_sig = &checkpoint.auth_sig().signature;
        Self {
            sequence_number: checkpoint.sequence_number,
            checkpoint_digest: *checkpoint.digest(),
            epoch: checkpoint.epoch,
            tx_digests,
            previous_checkpoint_digest: checkpoint.previous_digest,
            end_of_epoch_data: checkpoint.end_of_epoch_data.clone(),
            end_of_epoch: checkpoint.end_of_epoch_data.clone().is_some(),
            total_gas_cost,
            computation_cost: checkpoint.epoch_rolling_gas_cost_summary.computation_cost,
            storage_cost: checkpoint.epoch_rolling_gas_cost_summary.storage_cost,
            storage_rebate: checkpoint.epoch_rolling_gas_cost_summary.storage_rebate,
            non_refundable_storage_fee: checkpoint
                .epoch_rolling_gas_cost_summary
                .non_refundable_storage_fee,
            successful_tx_num,
            network_total_transactions: checkpoint.network_total_transactions,
            timestamp_ms: checkpoint.timestamp_ms,
            validator_signature: auth_sig.clone(),
            checkpoint_commitments: checkpoint.checkpoint_commitments.clone(),
            min_tx_sequence_number,
            max_tx_sequence_number,
            certified_checkpoint: Some(checkpoint.clone()),
            checkpoint_contents: Some(contents.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexedEvent {
    pub tx_sequence_number: u64,
    pub event_sequence_number: u64,
    pub checkpoint_sequence_number: u64,
    pub transaction_digest: TransactionDigest,
    pub sender: SuiAddress,
    pub package: ObjectID,
    pub module: String,
    pub event_type: String,
    pub event_type_package: ObjectID,
    pub event_type_module: String,
    /// Struct name of the event, without type parameters.
    pub event_type_name: String,
    pub bcs: Vec<u8>,
    pub timestamp_ms: u64,
}

impl IndexedEvent {
    pub fn from_event(
        tx_sequence_number: u64,
        event_sequence_number: u64,
        checkpoint_sequence_number: u64,
        transaction_digest: TransactionDigest,
        event: &sui_types::event::Event,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            tx_sequence_number,
            event_sequence_number,
            checkpoint_sequence_number,
            transaction_digest,
            sender: event.sender,
            package: event.package_id,
            module: event.transaction_module.to_string(),
            event_type: event.type_.to_canonical_string(/* with_prefix */ true),
            event_type_package: event.type_.address.into(),
            event_type_module: event.type_.module.to_string(),
            event_type_name: event.type_.name.to_string(),
            bcs: event.contents.clone(),
            timestamp_ms,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventIndex {
    pub tx_sequence_number: u64,
    pub event_sequence_number: u64,
    pub sender: SuiAddress,
    pub emit_package: ObjectID,
    pub emit_module: String,
    pub type_package: ObjectID,
    pub type_module: String,
    /// Struct name of the event, without type parameters.
    pub type_name: String,
    /// Type instantiation of the event, with type name and type parameters, if any.
    pub type_instantiation: String,
}

// for ingestion test
impl EventIndex {
    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        EventIndex {
            tx_sequence_number: rng.gen(),
            event_sequence_number: rng.gen(),
            sender: SuiAddress::random_for_testing_only(),
            emit_package: ObjectID::random(),
            emit_module: rng.gen::<u64>().to_string(),
            type_package: ObjectID::random(),
            type_module: rng.gen::<u64>().to_string(),
            type_name: rng.gen::<u64>().to_string(),
            type_instantiation: rng.gen::<u64>().to_string(),
        }
    }
}

impl EventIndex {
    pub fn from_event(
        tx_sequence_number: u64,
        event_sequence_number: u64,
        event: &sui_types::event::Event,
    ) -> Self {
        let type_instantiation = event
            .type_
            .to_canonical_string(/* with_prefix */ true)
            .splitn(3, "::")
            .collect::<Vec<_>>()[2]
            .to_string();
        Self {
            tx_sequence_number,
            event_sequence_number,
            sender: event.sender,
            emit_package: event.package_id,
            emit_module: event.transaction_module.to_string(),
            type_package: event.type_.address.into(),
            type_module: event.type_.module.to_string(),
            type_name: event.type_.name.to_string(),
            type_instantiation,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum OwnerType {
    Immutable = 0,
    Address = 1,
    Object = 2,
    Shared = 3,
}

pub enum ObjectStatus {
    Active = 0,
    WrappedOrDeleted = 1,
}

impl TryFrom<i16> for ObjectStatus {
    type Error = IndexerError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => ObjectStatus::Active,
            1 => ObjectStatus::WrappedOrDeleted,
            value => {
                return Err(IndexerError::PersistentStorageDataCorruptionError(format!(
                    "{value} as ObjectStatus"
                )))
            }
        })
    }
}

impl TryFrom<i16> for OwnerType {
    type Error = IndexerError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => OwnerType::Immutable,
            1 => OwnerType::Address,
            2 => OwnerType::Object,
            3 => OwnerType::Shared,
            value => {
                return Err(IndexerError::PersistentStorageDataCorruptionError(format!(
                    "{value} as OwnerType"
                )))
            }
        })
    }
}

// Returns owner_type, owner_address
pub fn owner_to_owner_info(owner: &Owner) -> (OwnerType, Option<SuiAddress>) {
    match owner {
        Owner::AddressOwner(address) => (OwnerType::Address, Some(*address)),
        Owner::ObjectOwner(address) => (OwnerType::Object, Some(*address)),
        Owner::Shared { .. } => (OwnerType::Shared, None),
        Owner::Immutable => (OwnerType::Immutable, None),
        // ConsensusV2 objects are treated as singly-owned for now in indexers.
        // This will need to be updated if additional Authenticators are added.
        Owner::ConsensusV2 { authenticator, .. } => {
            (OwnerType::Address, Some(*authenticator.as_single_owner()))
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum DynamicFieldKind {
    DynamicField = 0,
    DynamicObject = 1,
}

#[derive(Clone, Debug)]
pub struct IndexedObject {
    pub checkpoint_sequence_number: CheckpointSequenceNumber,
    pub object: Object,
    pub df_kind: Option<DynamicFieldType>,
}

impl IndexedObject {
    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        let random_address = SuiAddress::random_for_testing_only();
        IndexedObject {
            checkpoint_sequence_number: rng.gen(),
            object: Object::with_owner_for_testing(random_address),
            df_kind: {
                let random_value = rng.gen_range(0..3);
                match random_value {
                    0 => Some(DynamicFieldType::DynamicField),
                    1 => Some(DynamicFieldType::DynamicObject),
                    _ => None,
                }
            },
        }
    }
}

impl IndexedObject {
    pub fn from_object(
        checkpoint_sequence_number: CheckpointSequenceNumber,
        object: Object,
        df_kind: Option<DynamicFieldType>,
    ) -> Self {
        Self {
            checkpoint_sequence_number,
            object,
            df_kind,
        }
    }
}

#[derive(Clone, Debug)]
pub struct IndexedDeletedObject {
    pub object_id: ObjectID,
    pub object_version: u64,
    pub checkpoint_sequence_number: u64,
}

impl IndexedDeletedObject {
    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        IndexedDeletedObject {
            object_id: ObjectID::random(),
            object_version: rng.gen(),
            checkpoint_sequence_number: rng.gen(),
        }
    }
}

#[derive(Debug)]
pub struct IndexedPackage {
    pub package_id: ObjectID,
    pub move_package: MovePackage,
    pub checkpoint_sequence_number: u64,
}

#[derive(Debug, Clone)]
pub enum TransactionKind {
    SystemTransaction = 0,
    ProgrammableTransaction = 1,
}

#[derive(Debug, Clone)]
pub struct IndexedTransaction {
    pub tx_sequence_number: u64,
    pub tx_digest: TransactionDigest,
    pub sender_signed_data: SenderSignedData,
    pub effects: TransactionEffects,
    pub checkpoint_sequence_number: u64,
    pub timestamp_ms: u64,
    pub object_changes: Vec<IndexedObjectChange>,
    pub balance_change: Vec<sui_json_rpc_types::BalanceChange>,
    pub events: Vec<sui_types::event::Event>,
    pub transaction_kind: TransactionKind,
    pub successful_tx_num: u64,
}

#[derive(Debug, Clone)]
pub struct TxIndex {
    pub tx_sequence_number: u64,
    pub tx_kind: TransactionKind,
    pub transaction_digest: TransactionDigest,
    pub checkpoint_sequence_number: u64,
    pub input_objects: Vec<ObjectID>,
    pub changed_objects: Vec<ObjectID>,
    pub affected_objects: Vec<ObjectID>,
    pub payers: Vec<SuiAddress>,
    pub sender: SuiAddress,
    pub recipients: Vec<SuiAddress>,
    pub move_calls: Vec<(ObjectID, String, String)>,
}

impl TxIndex {
    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        TxIndex {
            tx_sequence_number: rng.gen(),
            tx_kind: if rng.gen_bool(0.5) {
                TransactionKind::SystemTransaction
            } else {
                TransactionKind::ProgrammableTransaction
            },
            transaction_digest: TransactionDigest::random(),
            checkpoint_sequence_number: rng.gen(),
            input_objects: (0..1000).map(|_| ObjectID::random()).collect(),
            changed_objects: (0..1000).map(|_| ObjectID::random()).collect(),
            affected_objects: (0..1000).map(|_| ObjectID::random()).collect(),
            payers: (0..rng.gen_range(0..100))
                .map(|_| SuiAddress::random_for_testing_only())
                .collect(),
            sender: SuiAddress::random_for_testing_only(),
            recipients: (0..rng.gen_range(0..1000))
                .map(|_| SuiAddress::random_for_testing_only())
                .collect(),
            move_calls: (0..rng.gen_range(0..1000))
                .map(|_| {
                    (
                        ObjectID::random(),
                        rng.gen::<u64>().to_string(),
                        rng.gen::<u64>().to_string(),
                    )
                })
                .collect(),
        }
    }
}

// ObjectChange is not bcs deserializable, IndexedObjectChange is.
#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum IndexedObjectChange {
    Published {
        package_id: ObjectID,
        version: SequenceNumber,
        digest: ObjectDigest,
        modules: Vec<String>,
    },
    Transferred {
        sender: SuiAddress,
        recipient: Owner,
        #[serde_as(as = "SuiStructTag")]
        object_type: StructTag,
        object_id: ObjectID,
        version: SequenceNumber,
        digest: ObjectDigest,
    },
    /// Object mutated.
    Mutated {
        sender: SuiAddress,
        owner: Owner,
        #[serde_as(as = "SuiStructTag")]
        object_type: StructTag,
        object_id: ObjectID,
        version: SequenceNumber,
        previous_version: SequenceNumber,
        digest: ObjectDigest,
    },
    /// Delete object
    Deleted {
        sender: SuiAddress,
        #[serde_as(as = "SuiStructTag")]
        object_type: StructTag,
        object_id: ObjectID,
        version: SequenceNumber,
    },
    /// Wrapped object
    Wrapped {
        sender: SuiAddress,
        #[serde_as(as = "SuiStructTag")]
        object_type: StructTag,
        object_id: ObjectID,
        version: SequenceNumber,
    },
    /// New object creation
    Created {
        sender: SuiAddress,
        owner: Owner,
        #[serde_as(as = "SuiStructTag")]
        object_type: StructTag,
        object_id: ObjectID,
        version: SequenceNumber,
        digest: ObjectDigest,
    },
}

impl From<ObjectChange> for IndexedObjectChange {
    fn from(oc: ObjectChange) -> Self {
        match oc {
            ObjectChange::Published {
                package_id,
                version,
                digest,
                modules,
            } => Self::Published {
                package_id,
                version,
                digest,
                modules,
            },
            ObjectChange::Transferred {
                sender,
                recipient,
                object_type,
                object_id,
                version,
                digest,
            } => Self::Transferred {
                sender,
                recipient,
                object_type,
                object_id,
                version,
                digest,
            },
            ObjectChange::Mutated {
                sender,
                owner,
                object_type,
                object_id,
                version,
                previous_version,
                digest,
            } => Self::Mutated {
                sender,
                owner,
                object_type,
                object_id,
                version,
                previous_version,
                digest,
            },
            ObjectChange::Deleted {
                sender,
                object_type,
                object_id,
                version,
            } => Self::Deleted {
                sender,
                object_type,
                object_id,
                version,
            },
            ObjectChange::Wrapped {
                sender,
                object_type,
                object_id,
                version,
            } => Self::Wrapped {
                sender,
                object_type,
                object_id,
                version,
            },
            ObjectChange::Created {
                sender,
                owner,
                object_type,
                object_id,
                version,
                digest,
            } => Self::Created {
                sender,
                owner,
                object_type,
                object_id,
                version,
                digest,
            },
        }
    }
}

impl From<IndexedObjectChange> for ObjectChange {
    fn from(val: IndexedObjectChange) -> Self {
        match val {
            IndexedObjectChange::Published {
                package_id,
                version,
                digest,
                modules,
            } => ObjectChange::Published {
                package_id,
                version,
                digest,
                modules,
            },
            IndexedObjectChange::Transferred {
                sender,
                recipient,
                object_type,
                object_id,
                version,
                digest,
            } => ObjectChange::Transferred {
                sender,
                recipient,
                object_type,
                object_id,
                version,
                digest,
            },
            IndexedObjectChange::Mutated {
                sender,
                owner,
                object_type,
                object_id,
                version,
                previous_version,
                digest,
            } => ObjectChange::Mutated {
                sender,
                owner,
                object_type,
                object_id,
                version,
                previous_version,
                digest,
            },
            IndexedObjectChange::Deleted {
                sender,
                object_type,
                object_id,
                version,
            } => ObjectChange::Deleted {
                sender,
                object_type,
                object_id,
                version,
            },
            IndexedObjectChange::Wrapped {
                sender,
                object_type,
                object_id,
                version,
            } => ObjectChange::Wrapped {
                sender,
                object_type,
                object_id,
                version,
            },
            IndexedObjectChange::Created {
                sender,
                owner,
                object_type,
                object_id,
                version,
                digest,
            } => ObjectChange::Created {
                sender,
                owner,
                object_type,
                object_id,
                version,
                digest,
            },
        }
    }
}

// SuiTransactionBlockResponseWithOptions is only used on the reading path
pub struct SuiTransactionBlockResponseWithOptions {
    pub response: SuiTransactionBlockResponse,
    pub options: SuiTransactionBlockResponseOptions,
}

impl From<SuiTransactionBlockResponseWithOptions> for SuiTransactionBlockResponse {
    fn from(value: SuiTransactionBlockResponseWithOptions) -> Self {
        let SuiTransactionBlockResponseWithOptions { response, options } = value;

        SuiTransactionBlockResponse {
            digest: response.digest,
            transaction: options.show_input.then_some(response.transaction).flatten(),
            raw_transaction: options
                .show_raw_input
                .then_some(response.raw_transaction)
                .unwrap_or_default(),
            effects: options.show_effects.then_some(response.effects).flatten(),
            events: options.show_events.then_some(response.events).flatten(),
            object_changes: options
                .show_object_changes
                .then_some(response.object_changes)
                .flatten(),
            balance_changes: options
                .show_balance_changes
                .then_some(response.balance_changes)
                .flatten(),
            timestamp_ms: response.timestamp_ms,
            confirmed_local_execution: response.confirmed_local_execution,
            checkpoint: response.checkpoint,
            errors: vec![],
            raw_effects: options
                .show_raw_effects
                .then_some(response.raw_effects)
                .unwrap_or_default(),
        }
    }
}
