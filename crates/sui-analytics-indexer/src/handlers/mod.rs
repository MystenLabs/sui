// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use anyhow::{anyhow, Result};
use move_core_types::annotated_value::{MoveStruct, MoveTypeLayout};
use move_core_types::language_storage::{StructTag, TypeTag};

use sui_indexer::framework::Handler;
use sui_package_resolver::{PackageStore, Resolver};
use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::{Object, Owner};
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionDataAPI;

use crate::tables::{InputObjectKind, ObjectStatus, OwnerType};
use crate::FileType;

pub mod checkpoint_handler;
pub mod df_handler;
pub mod event_handler;
pub mod move_call_handler;
pub mod object_handler;
pub mod package_handler;
pub mod transaction_handler;
pub mod transaction_objects_handler;

#[async_trait::async_trait]
pub trait AnalyticsHandler<S>: Handler {
    /// Read back rows which are ready to be persisted. This function
    /// will be invoked by the analytics processor after every call to
    /// process_checkpoint
    fn read(&mut self) -> Result<Vec<S>>;
    /// Type of data being written by this processor i.e. checkpoint, object, etc
    fn file_type(&self) -> Result<FileType>;
}

fn initial_shared_version(object: &Object) -> Option<u64> {
    match object.owner {
        Owner::Shared {
            initial_shared_version,
        } => Some(initial_shared_version.value()),
        _ => None,
    }
}

fn get_owner_type(object: &Object) -> OwnerType {
    match object.owner {
        Owner::AddressOwner(_) => OwnerType::AddressOwner,
        Owner::ObjectOwner(_) => OwnerType::ObjectOwner,
        Owner::Shared { .. } => OwnerType::Shared,
        Owner::Immutable => OwnerType::Immutable,
    }
}

fn get_owner_address(object: &Object) -> Option<String> {
    match object.owner {
        Owner::AddressOwner(address) => Some(address.to_string()),
        Owner::ObjectOwner(address) => Some(address.to_string()),
        Owner::Shared { .. } => None,
        Owner::Immutable => None,
    }
}

// Helper class to track input object kind.
// Build sets of object ids for input, shared input and gas coin objects as defined
// in the transaction data.
// Input objects include coins and shared.
struct InputObjectTracker {
    shared: BTreeSet<ObjectID>,
    coins: BTreeSet<ObjectID>,
    input: BTreeSet<ObjectID>,
}

impl InputObjectTracker {
    fn new(txn_data: &TransactionData) -> Self {
        let shared: BTreeSet<ObjectID> = txn_data
            .shared_input_objects()
            .iter()
            .map(|shared_io| shared_io.id())
            .collect();
        let coins: BTreeSet<ObjectID> = txn_data.gas().iter().map(|obj_ref| obj_ref.0).collect();
        let input: BTreeSet<ObjectID> = txn_data
            .input_objects()
            .expect("Input objects must be valid")
            .iter()
            .map(|io_kind| io_kind.object_id())
            .collect();
        Self {
            shared,
            coins,
            input,
        }
    }

    fn get_input_object_kind(&self, object_id: &ObjectID) -> Option<InputObjectKind> {
        if self.coins.contains(object_id) {
            Some(InputObjectKind::GasCoin)
        } else if self.shared.contains(object_id) {
            Some(InputObjectKind::SharedInput)
        } else if self.input.contains(object_id) {
            Some(InputObjectKind::Input)
        } else {
            None
        }
    }
}

// Helper class to track object status.
// Build sets of object ids for created, mutated and deleted objects as reported
// in the transaction effects.
struct ObjectStatusTracker {
    created: BTreeSet<ObjectID>,
    mutated: BTreeSet<ObjectID>,
    deleted: BTreeSet<ObjectID>,
}

impl ObjectStatusTracker {
    fn new(effects: &TransactionEffects) -> Self {
        let created: BTreeSet<ObjectID> = effects
            .created()
            .iter()
            .map(|(obj_ref, _)| obj_ref.0)
            .collect();
        let mutated: BTreeSet<ObjectID> = effects
            .mutated()
            .iter()
            .chain(effects.unwrapped().iter())
            .map(|(obj_ref, _)| obj_ref.0)
            .collect();
        let deleted: BTreeSet<ObjectID> = effects
            .all_tombstones()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        Self {
            created,
            mutated,
            deleted,
        }
    }

    fn get_object_status(&self, object_id: &ObjectID) -> Option<ObjectStatus> {
        if self.mutated.contains(object_id) {
            Some(ObjectStatus::Created)
        } else if self.deleted.contains(object_id) {
            Some(ObjectStatus::Mutated)
        } else if self.created.contains(object_id) {
            Some(ObjectStatus::Deleted)
        } else {
            None
        }
    }
}

async fn get_move_struct<T: PackageStore>(
    struct_tag: &StructTag,
    contents: &[u8],
    resolver: &Resolver<T>,
) -> Result<MoveStruct> {
    let move_struct = match resolver
        .type_layout(TypeTag::Struct(Box::new(struct_tag.clone())))
        .await?
    {
        MoveTypeLayout::Struct(move_struct_layout) => {
            MoveStruct::simple_deserialize(contents, &move_struct_layout)
        }
        _ => Err(anyhow!("Object is not a move struct")),
    }?;
    Ok(move_struct)
}
