// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    accumulator_event::AccumulatorEvent,
    base_types::{ObjectID, ObjectRef, SequenceNumber},
    digests::{ObjectDigest, TransactionDigest},
    event::Event,
    is_system_package,
    object::{Data, Object, Owner},
    storage::{BackingPackageStore, ObjectChange},
    transaction::{Argument, Command},
    type_input::TypeInput,
};
use move_core_types::language_storage::TypeTag;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

/// A type containing all of the information needed to work in execution with an object whose
/// consensus stream is ended, and when committing the execution effects of the transaction.
/// This holds:
/// 0. The object ID.
/// 1. The version.
/// 2. Whether the object appeared as mutable (or owned) in the transaction, or as read-only.
/// 3. The transaction digest of the previous transaction that used this object mutably or
///    took it by value.
pub type ConsensusStreamEndedInfo = (ObjectID, SequenceNumber, bool, TransactionDigest);

/// A sequence of information about removed consensus objects in the transaction's inputs.
pub type ConsensusStreamEndedObjects = Vec<ConsensusStreamEndedInfo>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SharedInput {
    Existing(ObjectRef),
    ConsensusStreamEnded(ConsensusStreamEndedInfo),
    Cancelled((ObjectID, SequenceNumber)),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct DynamicallyLoadedObjectMetadata {
    pub version: SequenceNumber,
    pub digest: ObjectDigest,
    pub owner: Owner,
    pub storage_rebate: u64,
    pub previous_transaction: TransactionDigest,
}

/// View of the store necessary to produce the layouts of types.
pub trait TypeLayoutStore: BackingPackageStore {}
impl<T> TypeLayoutStore for T where T: BackingPackageStore {}

#[derive(Debug)]
pub enum ExecutionResults {
    V1(ExecutionResultsV1),
    V2(ExecutionResultsV2),
}

#[derive(Debug)]
pub struct ExecutionResultsV1 {
    pub object_changes: BTreeMap<ObjectID, ObjectChange>,
    pub user_events: Vec<Event>,
}

/// Used by sui-execution v1 and above, to capture the execution results from Move.
/// The results represent the primitive information that can then be used to construct
/// both transaction effects V1 and V2.
#[derive(Debug, Default)]
pub struct ExecutionResultsV2 {
    /// All objects written regardless of whether they were mutated, created, or unwrapped.
    pub written_objects: BTreeMap<ObjectID, Object>,
    /// All objects that existed prior to this transaction, and are modified in this transaction.
    /// This includes any type of modification, including mutated, wrapped and deleted objects.
    pub modified_objects: BTreeSet<ObjectID>,
    /// All object IDs created in this transaction.
    pub created_object_ids: BTreeSet<ObjectID>,
    /// All object IDs deleted in this transaction.
    /// No object ID should be in both created_object_ids and deleted_object_ids.
    pub deleted_object_ids: BTreeSet<ObjectID>,
    /// All Move events emitted in this transaction.
    pub user_events: Vec<Event>,
    /// All accumulator events emitted in this transaction.
    pub accumulator_events: Vec<AccumulatorEvent>,
}

pub type ExecutionResult = (
    /*  mutable_reference_outputs */ Vec<(Argument, Vec<u8>, TypeTag)>,
    /*  return_values */ Vec<(Vec<u8>, TypeTag)>,
);

impl ExecutionResultsV2 {
    pub fn drop_writes(&mut self) {
        self.written_objects.clear();
        self.modified_objects.clear();
        self.created_object_ids.clear();
        self.deleted_object_ids.clear();
        self.user_events.clear();
        self.accumulator_events.clear();
    }

    pub fn merge_results(&mut self, new_results: Self) {
        self.written_objects.extend(new_results.written_objects);
        self.modified_objects.extend(new_results.modified_objects);
        self.created_object_ids
            .extend(new_results.created_object_ids);
        self.deleted_object_ids
            .extend(new_results.deleted_object_ids);
        self.user_events.extend(new_results.user_events);
        self.accumulator_events
            .extend(new_results.accumulator_events);
    }

    pub fn update_version_and_previous_tx(
        &mut self,
        lamport_version: SequenceNumber,
        prev_tx: TransactionDigest,
        input_objects: &BTreeMap<ObjectID, Object>,
        reshare_at_initial_version: bool,
    ) {
        for (id, obj) in self.written_objects.iter_mut() {
            // TODO: We can now get rid of the following logic by passing in lamport version
            // into the execution layer, and create new objects using the lamport version directly.

            // Update the version for the written object.
            match &mut obj.data {
                Data::Move(obj) => {
                    // Move objects all get the transaction's lamport timestamp
                    obj.increment_version_to(lamport_version);
                }

                Data::Package(pkg) => {
                    // Modified packages get their version incremented (this is a special case that
                    // only applies to system packages).  All other packages can only be created,
                    // and they are left alone.
                    if self.modified_objects.contains(id) {
                        debug_assert!(is_system_package(*id));
                        pkg.increment_version();
                    }
                }
            }

            // Record the version that the shared object was created at in its owner field.  Note,
            // this only works because shared objects must be created as shared (not created as
            // owned in one transaction and later converted to shared in another).
            if let Owner::Shared {
                initial_shared_version,
            } = &mut obj.owner
            {
                if self.created_object_ids.contains(id) {
                    assert_eq!(
                        *initial_shared_version,
                        SequenceNumber::new(),
                        "Initial version should be blank before this point for {id:?}",
                    );
                    *initial_shared_version = lamport_version;
                }

                // Update initial_shared_version for reshared objects
                if reshare_at_initial_version {
                    if let Some(Owner::Shared {
                        initial_shared_version: previous_initial_shared_version,
                    }) = input_objects.get(id).map(|obj| &obj.owner)
                    {
                        debug_assert!(!self.created_object_ids.contains(id));
                        debug_assert!(!self.deleted_object_ids.contains(id));
                        debug_assert!(
                            *initial_shared_version == SequenceNumber::new()
                                || *initial_shared_version == *previous_initial_shared_version
                        );

                        *initial_shared_version = *previous_initial_shared_version;
                    }
                }
            }

            // Record start version for ConsensusAddressOwner objects.
            if let Owner::ConsensusAddressOwner {
                start_version,
                owner,
            } = &mut obj.owner
            {
                debug_assert!(!self.deleted_object_ids.contains(id));

                if let Some(Owner::ConsensusAddressOwner {
                    start_version: previous_start_version,
                    owner: previous_owner,
                }) = input_objects.get(id).map(|obj| &obj.owner)
                {
                    if owner == previous_owner {
                        // Assign existing start_version in case a ConsensusAddressOwner object was
                        // transferred to the same owner.
                        *start_version = *previous_start_version;
                    } else {
                        // If owner changes, we need to begin a new stream.
                        *start_version = lamport_version;
                    }
                } else {
                    // ConsensusAddressOwner object was created, transferred from another Owner
                    // type, or unwrapped, so we begin a new stream.
                    *start_version = lamport_version;
                }
            }

            obj.previous_transaction = prev_tx;
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub enum ExecutionTimeObservationKey {
    // Contains all the fields from `ProgrammableMoveCall` besides `arguments`.
    MoveEntryPoint {
        /// The package containing the module and function.
        package: ObjectID,
        /// The specific module in the package containing the function.
        module: String,
        /// The function to be called.
        function: String,
        /// The type arguments to the function.
        /// NOTE: This field is currently not populated.
        type_arguments: Vec<TypeInput>,
    },
    TransferObjects,
    SplitCoins,
    MergeCoins,
    Publish, // special case: should not be used; we only use hard-coded estimate for this
    MakeMoveVec,
    Upgrade,
}

impl std::fmt::Display for ExecutionTimeObservationKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionTimeObservationKey::MoveEntryPoint {
                module, function, ..
            } => {
                write!(f, "{}:{}", module, function)
            }
            ExecutionTimeObservationKey::TransferObjects => write!(f, "TransferObjects"),
            ExecutionTimeObservationKey::SplitCoins => write!(f, "SplitCoins"),
            ExecutionTimeObservationKey::MergeCoins => write!(f, "MergeCoins"),
            ExecutionTimeObservationKey::Publish => write!(f, "Publish"),
            ExecutionTimeObservationKey::MakeMoveVec => write!(f, "MakeMoveVec"),
            ExecutionTimeObservationKey::Upgrade => write!(f, "Upgrade"),
        }
    }
}

impl ExecutionTimeObservationKey {
    pub fn is_move_call(&self) -> bool {
        matches!(self, ExecutionTimeObservationKey::MoveEntryPoint { .. })
    }

    pub fn from_command(command: &Command) -> Self {
        match command {
            Command::MoveCall(call) => ExecutionTimeObservationKey::MoveEntryPoint {
                package: call.package,
                module: call.module.clone(),
                function: call.function.clone(),
                type_arguments: vec![],
            },
            Command::TransferObjects(_, _) => ExecutionTimeObservationKey::TransferObjects,
            Command::SplitCoins(_, _) => ExecutionTimeObservationKey::SplitCoins,
            Command::MergeCoins(_, _) => ExecutionTimeObservationKey::MergeCoins,
            Command::Publish(_, _) => ExecutionTimeObservationKey::Publish,
            Command::MakeMoveVec(_, _) => ExecutionTimeObservationKey::MakeMoveVec,
            Command::Upgrade(_, _, _, _) => ExecutionTimeObservationKey::Upgrade,
        }
    }

    pub fn default_duration(&self) -> Duration {
        match self {
            ExecutionTimeObservationKey::MoveEntryPoint { .. } => Duration::from_millis(1),
            ExecutionTimeObservationKey::TransferObjects => Duration::from_millis(1),
            ExecutionTimeObservationKey::SplitCoins => Duration::from_millis(1),
            ExecutionTimeObservationKey::MergeCoins => Duration::from_millis(1),
            ExecutionTimeObservationKey::Publish => Duration::from_millis(3),
            ExecutionTimeObservationKey::MakeMoveVec => Duration::from_millis(1),
            ExecutionTimeObservationKey::Upgrade => Duration::from_millis(3),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ExecutionTiming {
    Success(Duration),
    Abort(Duration),
}

impl ExecutionTiming {
    pub fn is_abort(&self) -> bool {
        matches!(self, ExecutionTiming::Abort(_))
    }

    pub fn duration(&self) -> Duration {
        match self {
            ExecutionTiming::Success(duration) => *duration,
            ExecutionTiming::Abort(duration) => *duration,
        }
    }
}

pub type ResultWithTimings<R, E> = Result<(R, Vec<ExecutionTiming>), (E, Vec<ExecutionTiming>)>;
