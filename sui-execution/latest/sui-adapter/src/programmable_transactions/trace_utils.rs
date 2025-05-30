// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module implements support for tracing related to PTB execution. IMPORTANT:
//! Bodies of all public functions in this module should be enclosed in a large if statement checking if
//! tracing is enabled or not to make sure that any errors coming from these functions only manifest itself
//! when tracing is enabled.

use crate::execution_value::{ObjectContents, ObjectValue, Value};
use crate::programmable_transactions::context::*;
use move_core_types::language_storage::StructTag;
use move_core_types::{identifier::Identifier, language_storage::TypeTag};
use move_trace_format::{
    format::{MoveTraceBuilder, RefType, TraceEvent, TypeTagWithRefs},
    value::{SerializableMoveValue, SimplifiedMoveStruct},
};
use move_vm_types::loaded_data::runtime_types::Type;
use sui_types::coin::Coin;
use sui_types::object::bounded_visitor::BoundedVisitor;
use sui_types::ptb_trace::{PTBCommandInfo, PTBExternalEvent, SplitCoinsEvent, TransferEvent};
use sui_types::transaction::Command;
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionError, ExecutionErrorKind},
    ptb_trace::MoveValueInfo,
};

/// Inserts Move call start event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_move_call_start(trace_builder_opt: &mut Option<MoveTraceBuilder>) {
    if let Some(trace_builder) = trace_builder_opt {
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBExternalEvent::MoveCallStart
        ))));
    }
}

/// Inserts Move call end event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_move_call_end(trace_builder_opt: &mut Option<MoveTraceBuilder>) {
    if let Some(trace_builder) = trace_builder_opt {
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBExternalEvent::MoveCallEnd
        ))));
    }
}

/// Inserts transfer event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_transfer(
    context: &mut ExecutionContext<'_, '_, '_>,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    obj_values: &[ObjectValue],
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        let to_transfer = obj_values
            .iter()
            .map(|v| obj_info_from_obj_value(context, v))
            .collect::<Result<Vec<_>, _>>()?;
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBExternalEvent::TransferObjects(TransferEvent { to_transfer })
        ))));
    }
    Ok(())
}

/// Inserts PTB summary event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_ptb_summary(
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    commands: &[Command],
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBExternalEvent::Summary(
                commands
                    .iter()
                    .map(|c| match c {
                        Command::MoveCall(move_call) => {
                            let pkg = move_call.package.to_string();
                            let module = move_call.module.clone();
                            let function = move_call.function.clone();
                            PTBCommandInfo::MoveCall {
                                pkg,
                                module,
                                function,
                            }
                        }
                        Command::TransferObjects(..) => PTBCommandInfo::TransferObjects,
                        Command::SplitCoins(..) => PTBCommandInfo::SplitCoins,
                        Command::MergeCoins(..) => PTBCommandInfo::MergeCoins,
                        Command::Publish(..) => PTBCommandInfo::Publish,
                        Command::MakeMoveVec(..) => PTBCommandInfo::MakeMoveVec,
                        Command::Upgrade(..) => PTBCommandInfo::Upgrade,
                    })
                    .collect(),
            )
        ))));
    }

    Ok(())
}

/// Inserts split coins event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_split_coins(
    context: &mut ExecutionContext<'_, '_, '_>,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    coin_type: &Type,
    input_coin: &Coin,
    split_coin_values: &[Value],
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        let type_tag_with_refs = trace_type_to_type_tag_with_refs(context, coin_type)?;
        let split_coin_infos = split_coin_values
            .iter()
            .map(|coin_val| {
                let Value::Object(ObjectValue {
                    contents: ObjectContents::Coin(split_coin),
                    ..
                }) = coin_val
                else {
                    invariant_violation!("Expected result of split coins PTB command to be a coin");
                };
                coin_obj_info(
                    type_tag_with_refs.clone(),
                    *split_coin.id.object_id(),
                    split_coin.balance.value(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let input = coin_obj_info(
            type_tag_with_refs.clone(),
            *input_coin.id.object_id(),
            input_coin.value(),
        )?;
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBExternalEvent::SplitCoins(SplitCoinsEvent {
                input,
                result: split_coin_infos,
            })
        ))));
    }
    Ok(())
}

/// Creates `ObjectInfo` from `ObjectValue`.
fn obj_info_from_obj_value(
    context: &mut ExecutionContext<'_, '_, '_>,
    obj_val: &ObjectValue,
) -> Result<MoveValueInfo, ExecutionError> {
    let type_tag_with_refs = trace_type_to_type_tag_with_refs(context, &obj_val.type_)?;
    match &obj_val.contents {
        ObjectContents::Coin(coin) => {
            coin_obj_info(type_tag_with_refs, *coin.id.object_id(), coin.value())
        }
        ObjectContents::Raw(bytes) => {
            let layout = context
                .vm
                .get_runtime()
                .type_to_fully_annotated_layout(&obj_val.type_)
                .map_err(|e| {
                    ExecutionError::new_with_source(ExecutionErrorKind::InvariantViolation, e)
                })?;
            let move_value = BoundedVisitor::deserialize_value(bytes, &layout).map_err(|e| {
                ExecutionError::new_with_source(ExecutionErrorKind::InvariantViolation, e)
            })?;
            let serialized_move_value = SerializableMoveValue::from(move_value);
            Ok(MoveValueInfo {
                type_: type_tag_with_refs,
                value: serialized_move_value,
            })
        }
    }
}

/// Creates `ObjectInfo` for a coin.
fn coin_obj_info(
    type_tag_with_refs: TypeTagWithRefs,
    object_id: ObjectID,
    balance: u64,
) -> Result<MoveValueInfo, ExecutionError> {
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
    Ok(MoveValueInfo {
        type_: type_tag_with_refs,
        value: SerializableMoveValue::Struct(coin_obj),
    })
}

/// Converts a type to type tag format used in tracing.
fn trace_type_to_type_tag_with_refs(
    context: &mut ExecutionContext<'_, '_, '_>,
    type_: &Type,
) -> Result<TypeTagWithRefs, ExecutionError> {
    let (deref_type, ref_type) = match type_ {
        Type::Reference(t) => (t.as_ref(), Some(RefType::Imm)),
        Type::MutableReference(t) => (t.as_ref(), Some(RefType::Mut)),
        t => (t, None),
    };
    let type_ = context
        .vm
        .get_runtime()
        .get_type_tag(deref_type)
        .map_err(|e| ExecutionError::new_with_source(ExecutionErrorKind::InvariantViolation, e))?;
    Ok(TypeTagWithRefs { type_, ref_type })
}
