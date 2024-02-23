// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};
use move_core_types::annotated_value::{MoveStruct, MoveTypeLayout};
use move_core_types::language_storage::{StructTag, TypeTag};

use sui_indexer::framework::Handler;
use sui_json_rpc_types::{SuiMoveStruct, SuiMoveValue};
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
pub mod wrapped_object_handler;

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
            Some(ObjectStatus::Mutated)
        } else if self.deleted.contains(object_id) {
            Some(ObjectStatus::Deleted)
        } else if self.created.contains(object_id) {
            Some(ObjectStatus::Created)
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

#[derive(Debug, Default)]
pub struct WrappedStruct {
    object_id: Option<ObjectID>,
    struct_tag: Option<StructTag>,
}

fn parse_struct_field(
    path: &str,
    sui_move_value: SuiMoveValue,
    curr_struct: &mut WrappedStruct,
    all_structs: &mut BTreeMap<String, WrappedStruct>,
) {
    match sui_move_value {
        SuiMoveValue::Struct(move_struct) => parse_struct(path, move_struct, all_structs),
        SuiMoveValue::Vector(fields) => {
            for (index, field) in fields.iter().enumerate() {
                parse_struct_field(
                    &format!("{}[{}]", path, &index),
                    field.clone(),
                    curr_struct,
                    all_structs,
                );
            }
        }
        SuiMoveValue::Option(option_sui_move_value) => {
            if option_sui_move_value.is_some() {
                parse_struct_field(
                    path,
                    option_sui_move_value.unwrap(),
                    curr_struct,
                    all_structs,
                );
            }
        }
        SuiMoveValue::UID { id } => curr_struct.object_id = Some(id),
        _ => {}
    }
}

fn parse_struct(
    path: &str,
    sui_move_struct: SuiMoveStruct,
    all_structs: &mut BTreeMap<String, WrappedStruct>,
) {
    let mut wrapped_struct = WrappedStruct::default();
    match sui_move_struct {
        SuiMoveStruct::WithTypes { type_, fields } => {
            wrapped_struct.struct_tag = Some(type_);
            for (k, v) in fields {
                parse_struct_field(
                    &format!("{}.{}", path, &k),
                    v,
                    &mut wrapped_struct,
                    all_structs,
                );
            }
            all_structs.insert(path.to_string(), wrapped_struct);
        }
        SuiMoveStruct::WithFields(fields) => {
            for (k, v) in fields {
                parse_struct_field(
                    &format!("{}.{}", path, &k),
                    v,
                    &mut wrapped_struct,
                    all_structs,
                );
            }
            all_structs.insert(path.to_string(), wrapped_struct);
        }
        SuiMoveStruct::Runtime(values) => {
            for (index, field) in values.iter().enumerate() {
                parse_struct_field(
                    &format!("{}[{}]", path, &index),
                    field.clone(),
                    &mut wrapped_struct,
                    all_structs,
                );
            }
            all_structs.insert(path.to_string(), wrapped_struct);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::handlers::parse_struct;
    use std::collections::BTreeMap;
    use sui_json_rpc_types::SuiMoveStruct;
    use sui_types::base_types::ObjectID;

    #[tokio::test]
    async fn test_wrapped_object_parsing_simple() -> anyhow::Result<()> {
        let input = r#"{"x":{"y":{"id":{"id":"0x100"},"size":"15"},"id":{"id":"0x200"}},"id":{"id":"0x300"}}"#;
        let move_struct: SuiMoveStruct = serde_json::from_str(input).unwrap();
        let mut all_structs = BTreeMap::new();
        parse_struct("$", move_struct, &mut all_structs);
        assert_eq!(
            all_structs.get("$").unwrap().object_id,
            Some(ObjectID::from_hex_literal("0x300")?)
        );
        assert_eq!(
            all_structs.get("$.x").unwrap().object_id,
            Some(ObjectID::from_hex_literal("0x200")?)
        );
        assert_eq!(
            all_structs.get("$.x.y").unwrap().object_id,
            Some(ObjectID::from_hex_literal("0x100")?)
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_wrapped_object_parsing_with_array() -> anyhow::Result<()> {
        let input = r#"{"ema_prices":{"id":{"id":"0x100"},"size":"0"},"id":{"id":"0x200"},"prices":{"id":{"id":"0x300"},"size":"11"},"primary_price_update_policy":{"id":{"id":"0x400"},"rules":{"contents":[{"name":"910f30cbc7f601f75a5141a01265cd47c62d468707c5e1aecb32a18f448cb25a::rule::Rule"}]}},"secondary_price_update_policy":{"id":{"id":"0x500"},"rules":{"contents":[]}}}"#;
        let move_struct: SuiMoveStruct = serde_json::from_str(input).unwrap();
        let mut all_structs = BTreeMap::new();
        parse_struct("$", move_struct, &mut all_structs);
        assert_eq!(
            all_structs.get("$").unwrap().object_id,
            Some(ObjectID::from_hex_literal("0x200")?)
        );
        assert_eq!(
            all_structs.get("$.ema_prices").unwrap().object_id,
            Some(ObjectID::from_hex_literal("0x100")?)
        );
        assert_eq!(
            all_structs.get("$.prices").unwrap().object_id,
            Some(ObjectID::from_hex_literal("0x300")?)
        );
        assert_eq!(
            all_structs
                .get("$.primary_price_update_policy")
                .unwrap()
                .object_id,
            Some(ObjectID::from_hex_literal("0x400")?)
        );
        assert_eq!(
            all_structs
                .get("$.secondary_price_update_policy")
                .unwrap()
                .object_id,
            Some(ObjectID::from_hex_literal("0x500")?)
        );
        assert_eq!(
            all_structs
                .get("$.secondary_price_update_policy.rules")
                .unwrap()
                .object_id,
            None
        );
        assert_eq!(
            all_structs
                .get("$.primary_price_update_policy.rules")
                .unwrap()
                .object_id,
            None
        );
        assert_eq!(
            all_structs
                .get("$.primary_price_update_policy.rules.contents[0]")
                .unwrap()
                .object_id,
            None
        );
        Ok(())
    }
}
