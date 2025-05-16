// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::programmable_transactions::context::*;
use move_core_types::language_storage::StructTag;
use move_core_types::{identifier::Identifier, language_storage::TypeTag};
use move_trace_format::{
    format::{MoveTraceBuilder, RefType, TraceEvent, TypeTagWithRefs},
    value::{SerializableMoveValue, SimplifiedMoveStruct},
};
use move_vm_types::loaded_data::runtime_types::Type;
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionError, ExecutionErrorKind},
    ptb_trace::ObjectInfo,
};

/// Creates `ObjectInfor` for a coin.
pub fn coin_obj_info(
    type_tag_with_refs: TypeTagWithRefs,
    object_id: ObjectID,
    balance: u64,
) -> Result<ObjectInfo, ExecutionError> {
    let coin_type_tag = match type_tag_with_refs.type_.clone() {
        TypeTag::Struct(tag) => tag,
        _ => invariant_violation!("Expected a struct type tag when creating a Move coin value"),
    };
    // object.ID
    let object_id = SerializableMoveValue::Address(object_id.into());
    let object_id_struct_tag = StructTag {
        address: coin_type_tag.address,
        module: Identifier::new("object").unwrap(),
        name: Identifier::new("ID").unwrap(),
        type_params: vec![],
    };
    let object_id_struct = SimplifiedMoveStruct {
        type_: object_id_struct_tag,
        fields: vec![(Identifier::new("value").unwrap(), object_id)],
    };
    let serializable_object_id = SerializableMoveValue::Struct(object_id_struct);
    // object.UID
    let object_uid_struct_tag = StructTag {
        address: coin_type_tag.address,
        module: Identifier::new("object").unwrap(),
        name: Identifier::new("UID").unwrap(),
        type_params: vec![],
    };
    let object_uid_struct = SimplifiedMoveStruct {
        type_: object_uid_struct_tag,
        fields: vec![(Identifier::new("id").unwrap(), serializable_object_id)],
    };
    let serializable_object_uid = SerializableMoveValue::Struct(object_uid_struct);
    // coin.Balance
    let serializable_value = SerializableMoveValue::U64(balance);
    let balance_struct_tag = StructTag {
        address: coin_type_tag.address,
        module: Identifier::new("balance").unwrap(),
        name: Identifier::new("Balance").unwrap(),
        type_params: coin_type_tag.type_params.clone(),
    };
    let balance_struct = SimplifiedMoveStruct {
        type_: balance_struct_tag,
        fields: vec![(Identifier::new("value").unwrap(), serializable_value)],
    };
    let serializable_balance = SerializableMoveValue::Struct(balance_struct);
    // coin.Coin
    let coin_obj = SimplifiedMoveStruct {
        type_: *coin_type_tag,
        fields: vec![
            (Identifier::new("id").unwrap(), serializable_object_uid),
            (Identifier::new("balance").unwrap(), serializable_balance),
        ],
    };
    Ok(ObjectInfo {
        type_: type_tag_with_refs,
        value: SerializableMoveValue::Struct(coin_obj),
    })
}

/// Pushes event to the trace builder if tracing is enabled.
/// Event is created via `create_event` function taking a vector of 'TypeTagWithRef's
/// as an argument that is created from vector of `type_`s inside this function.
/// This somewhat complicated code structure was introduced to make sure
/// that the invariant violation that can result from tag creation failure
/// will only trigger when tracing is enabled.
pub fn push_trace_event_with_type_tags(
    context: &mut ExecutionContext<'_, '_, '_>,
    types: &[Type],
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    create_event: impl FnOnce(&mut Vec<TypeTagWithRefs>) -> Result<TraceEvent, ExecutionError>,
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        let mut type_tags_with_ref = types
            .iter()
            .map(|type_| {
                let (deref_type, ref_type) = match type_ {
                    Type::Reference(t) => (t.as_ref(), Some(RefType::Imm)),
                    Type::MutableReference(t) => (t.as_ref(), Some(RefType::Mut)),
                    t => (t, None),
                };
                // this invariant violation will only trigger when tracing
                context
                    .vm
                    .get_runtime()
                    .get_type_tag(deref_type)
                    .map(|type_tag_with_ref| TypeTagWithRefs {
                        type_: type_tag_with_ref,
                        ref_type,
                    })
                    .map_err(|e| {
                        ExecutionError::new_with_source(ExecutionErrorKind::InvariantViolation, e)
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        trace_builder.push_event(create_event(&mut type_tags_with_ref)?);
    }
    Ok(())
}

/// Converts a type to type tag format used in tracing.
/// SHOULD ONLY BE USED WHEN TRACING IS ENABLED as it may
/// cause invariant violation.
fn trace_type_to_type_tag_with_refs(
    context: &mut ExecutionContext<'_, '_, '_>,
    t: &Type,
) -> Result<TypeTagWithRefs, ExecutionError> {
    let (deref_type, ref_type) = match t {
        Type::Reference(t) => (t.as_ref(), Some(RefType::Imm)),
        Type::MutableReference(t) => (t.as_ref(), Some(RefType::Mut)),
        t => (t, None),
    };
    // this invariant violation will only trigger when tracing
    let type_ = context
        .vm
        .get_runtime()
        .get_type_tag(deref_type)
        .map_err(|e| ExecutionError::new_with_source(ExecutionErrorKind::InvariantViolation, e))?;
    Ok(TypeTagWithRefs { type_, ref_type })
}
