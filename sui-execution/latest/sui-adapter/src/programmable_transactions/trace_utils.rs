// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module implements support for tracing related to PTB execution. IMPORTANT:
//! Bodies of all public functions in this module should be enclosed in a large if statement checking if
//! tracing is enabled or not to make sure that any errors coming from these functions only manifest itself
//! when tracing is enabled.

use crate::{
    execution_mode::ExecutionMode,
    execution_value::{ObjectContents, ObjectValue, Value},
    programmable_transactions::context::*,
};
use move_core_types::{
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
};
use move_trace_format::{
    format::{Effect, MoveTraceBuilder, RefType, TraceEvent, TypeTagWithRefs},
    value::{SerializableMoveValue, SimplifiedMoveStruct},
};
use move_vm_types::loaded_data::runtime_types::Type;
use sui_types::{
    base_types::ObjectID,
    coin::Coin,
    error::{ExecutionError, ExecutionErrorKind},
    object::bounded_visitor::BoundedVisitor,
    ptb_trace::{
        ExtMoveValue, ExtMoveValueInfo, ExternalEvent, PTBCommandInfo, PTBEvent, SummaryEvent,
    },
    transaction::Command,
};
use sui_verifier::INIT_FN_NAME;

/// Inserts Move call start event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_move_call_start(trace_builder_opt: &mut Option<MoveTraceBuilder>) {
    if let Some(trace_builder) = trace_builder_opt {
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBEvent::MoveCallStart
        ))));
    }
}

/// Inserts Move call end event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_move_call_end(trace_builder_opt: &mut Option<MoveTraceBuilder>) {
    if let Some(trace_builder) = trace_builder_opt {
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBEvent::MoveCallEnd
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
        let mut to_transfer = vec![];
        for (idx, v) in obj_values.iter().enumerate() {
            let obj_info = move_value_info_from_obj_value(context, v)?;
            to_transfer.push(ExtMoveValue::Single {
                name: format!("obj{idx}"),
                info: obj_info,
            });
        }
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBEvent::ExternalEvent(ExternalEvent {
                description: "TransferObjects: obj0...objN => ()".to_string(),
                name: "Transfer".to_string(),
                values: to_transfer,
            })
        ))));
    }
    Ok(())
}

/// Inserts PTB summary event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_ptb_summary<Mode: ExecutionMode>(
    context: &mut ExecutionContext<'_, '_, '_>,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    commands: &[Command],
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        let events = commands
            .iter()
            .map(|c| match c {
                Command::MoveCall(move_call) => {
                    let pkg = move_call.package.to_string();
                    let module = move_call.module.clone();
                    let function = move_call.function.clone();
                    Ok(vec![PTBCommandInfo::MoveCall {
                        pkg,
                        module,
                        function,
                    }])
                }
                Command::TransferObjects(..) => Ok(vec![PTBCommandInfo::ExternalEvent(
                    "TransferObjects".to_string(),
                )]),
                Command::SplitCoins(..) => Ok(vec![PTBCommandInfo::ExternalEvent(
                    "SplitCoins".to_string(),
                )]),
                Command::MergeCoins(..) => Ok(vec![PTBCommandInfo::ExternalEvent(
                    "MergeCoins".to_string(),
                )]),
                Command::Publish(module_bytes, _) => {
                    let mut events = vec![];
                    events.push(PTBCommandInfo::ExternalEvent("Publish".to_string()));
                    // Not ideal but it only runs when tracing is enabled so overhead
                    // should be insignificant
                    let modules = context.deserialize_modules(module_bytes)?;
                    events.extend(modules.into_iter().find_map(|m| {
                        for fdef in &m.function_defs {
                            let fhandle = m.function_handle_at(fdef.function);
                            let fname = m.identifier_at(fhandle.name);
                            if fname == INIT_FN_NAME {
                                return Some(PTBCommandInfo::MoveCall {
                                    pkg: m.address().to_string(),
                                    module: m.name().to_string(),
                                    function: INIT_FN_NAME.to_string(),
                                });
                            }
                        }
                        None
                    }));
                    Ok(events)
                }
                Command::MakeMoveVec(..) => Ok(vec![PTBCommandInfo::ExternalEvent(
                    "MakeMoveVec".to_string(),
                )]),
                Command::Upgrade(..) => {
                    Ok(vec![PTBCommandInfo::ExternalEvent("Upgrade".to_string())])
                }
            })
            .collect::<Result<Vec<Vec<PTBCommandInfo>>, ExecutionError>>()?
            .into_iter()
            .flatten()
            .collect();
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBEvent::Summary(SummaryEvent {
                name: "PTBSummary".to_string(),
                events,
            })
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
        let mut split_coin_move_values = vec![];
        for coin_val in split_coin_values {
            let Value::Object(ObjectValue {
                contents: ObjectContents::Coin(coin),
                ..
            }) = coin_val
            else {
                invariant_violation!("Expected result of split coins PTB command to be a coin");
            };
            split_coin_move_values.push(
                coin_move_value_info(
                    type_tag_with_refs.clone(),
                    *coin.id.object_id(),
                    coin.balance.value(),
                )?
                .value,
            );
        }

        let input = coin_move_value_info(
            type_tag_with_refs.clone(),
            *input_coin.id.object_id(),
            input_coin.value(),
        )?;
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBEvent::ExternalEvent(ExternalEvent {
                description: "SplitCoins: input => result".to_string(),
                name: "SplitCoins".to_string(),
                values: vec![
                    ExtMoveValue::Single {
                        name: "input".to_string(),
                        info: input
                    },
                    ExtMoveValue::Vector {
                        name: "result".to_string(),
                        type_: type_tag_with_refs.clone(),
                        value: split_coin_move_values
                    },
                ],
            })
        ))));
    }
    Ok(())
}

/// Inserts merge coins event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_merge_coins(
    context: &mut ExecutionContext<'_, '_, '_>,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    coin_type: &Type,
    input_infos: &[(u64, ObjectID)],
    target_coin: &Coin,
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        let type_tag_with_refs = trace_type_to_type_tag_with_refs(context, coin_type)?;
        let mut input_coin_move_values = vec![];
        let mut to_merge = 0;
        for (balance, id) in input_infos {
            input_coin_move_values.push(coin_move_value_info(
                type_tag_with_refs.clone(),
                *id,
                *balance,
            )?);
            to_merge += balance;
        }
        let merge_target = coin_move_value_info(
            type_tag_with_refs.clone(),
            *target_coin.id.object_id(),
            target_coin.value() - to_merge,
        )?;
        let mut values = vec![ExtMoveValue::Single {
            name: "merge_target".to_string(),
            info: merge_target,
        }];
        for (idx, input_value) in input_coin_move_values.into_iter().enumerate() {
            values.push(ExtMoveValue::Single {
                name: format!("coin{idx}"),
                info: input_value,
            });
        }
        let merge_result = coin_move_value_info(
            type_tag_with_refs.clone(),
            *target_coin.id.object_id(),
            target_coin.value(),
        )?;
        values.push(ExtMoveValue::Single {
            name: "merge_result".to_string(),
            info: merge_result,
        });
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBEvent::ExternalEvent(ExternalEvent {
                description: "MergeCoins: merge_target, coin0...coinN => mergeresult".to_string(),
                name: "MergeCoins".to_string(),
                values,
            })
        ))));
    }
    Ok(())
}

/// Inserts make move vec event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_make_move_vec(
    context: &mut ExecutionContext<'_, '_, '_>,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    move_values: Vec<ExtMoveValueInfo>,
    type_: &Type,
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        let type_tag_with_refs = trace_type_to_type_tag_with_refs(context, type_)?;
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBEvent::ExternalEvent(ExternalEvent {
                description: "MakeMoveVec: vector".to_string(),
                name: "MakeMoveVec".to_string(),
                values: vec![ExtMoveValue::Vector {
                    name: "vector".to_string(),
                    type_: type_tag_with_refs,
                    value: move_values
                        .into_iter()
                        .map(|move_value| move_value.value)
                        .collect(),
                }],
            })
        ))));
    }
    Ok(())
}

/// Inserts publish event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_publish_event(
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBEvent::ExternalEvent(ExternalEvent {
                description: "Publish: ()".to_string(),
                name: "Publish".to_string(),
                values: vec![],
            })
        ))));
    }
    Ok(())
}

/// Inserts upgrade event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_upgrade_event(
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBEvent::ExternalEvent(ExternalEvent {
                description: "Upgrade: ()".to_string(),
                name: "Upgrade".to_string(),
                values: vec![],
            })
        ))));
    }
    Ok(())
}

/// Inserts execution error event into the trace. As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn trace_execution_error(trace_builder_opt: &mut Option<MoveTraceBuilder>, msg: String) {
    if let Some(trace_builder) = trace_builder_opt {
        trace_builder.push_event(TraceEvent::Effect(Box::new(Effect::ExecutionError(msg))));
    }
}

/// Adds `ExtMoveValueInfo` to the mutable vector passed as an argument.
/// As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn add_move_value_info_from_value(
    context: &mut ExecutionContext<'_, '_, '_>,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    move_values: &mut Vec<ExtMoveValueInfo>,
    type_: &Type,
    value: &Value,
) -> Result<(), ExecutionError> {
    if trace_builder_opt.is_some()
        && let Some(move_value_info) = move_value_info_from_value(context, type_, value)?
    {
        move_values.push(move_value_info);
    }
    Ok(())
}

/// Adds `ExtMoveValueInfo` to the mutable vector passed as an argument.
/// As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn add_move_value_info_from_obj_value(
    context: &mut ExecutionContext<'_, '_, '_>,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    move_values: &mut Vec<ExtMoveValueInfo>,
    obj_val: &ObjectValue,
) -> Result<(), ExecutionError> {
    if trace_builder_opt.is_some() {
        let move_value_info = move_value_info_from_obj_value(context, obj_val)?;
        move_values.push(move_value_info);
    }
    Ok(())
}

/// Adds coin object info to the mutable vector passed as an argument.
/// As is the case for all other public functions in this module,
/// its body is (and must be) enclosed in an if statement checking if tracing is enabled.
pub fn add_coin_obj_info(
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    coin_infos: &mut Vec<(u64, ObjectID)>,
    balance: u64,
    id: ObjectID,
) {
    if trace_builder_opt.is_some() {
        coin_infos.push((balance, id));
    }
}

/// Creates `ExtMoveValueInfo` from raw bytes.
fn move_value_info_from_raw_bytes(
    context: &mut ExecutionContext<'_, '_, '_>,
    type_: &Type,
    bytes: &[u8],
) -> Result<ExtMoveValueInfo, ExecutionError> {
    let type_tag_with_refs = trace_type_to_type_tag_with_refs(context, type_)?;
    let layout = context
        .vm
        .get_runtime()
        .type_to_fully_annotated_layout(type_)
        .map_err(|e| ExecutionError::new_with_source(ExecutionErrorKind::InvariantViolation, e))?;
    let move_value = BoundedVisitor::deserialize_value(bytes, &layout)
        .map_err(|e| ExecutionError::new_with_source(ExecutionErrorKind::InvariantViolation, e))?;
    let serialized_move_value = SerializableMoveValue::from(move_value);
    Ok(ExtMoveValueInfo {
        type_: type_tag_with_refs,
        value: serialized_move_value,
    })
}

/// Creates `ExtMoveValueInfo` from `Value`.
fn move_value_info_from_value(
    context: &mut ExecutionContext<'_, '_, '_>,
    type_: &Type,
    value: &Value,
) -> Result<Option<ExtMoveValueInfo>, ExecutionError> {
    match value {
        Value::Object(obj_val) => Ok(Some(move_value_info_from_obj_value(context, obj_val)?)),
        Value::Raw(_, bytes) => Ok(Some(move_value_info_from_raw_bytes(context, type_, bytes)?)),
        Value::Receiving(_, _, _) => Ok(None),
    }
}

/// Creates `ExtMoveValueInfo` from `ObjectValue`.
fn move_value_info_from_obj_value(
    context: &mut ExecutionContext<'_, '_, '_>,
    obj_val: &ObjectValue,
) -> Result<ExtMoveValueInfo, ExecutionError> {
    let type_tag_with_refs = trace_type_to_type_tag_with_refs(context, &obj_val.type_)?;
    match &obj_val.contents {
        ObjectContents::Coin(coin) => {
            coin_move_value_info(type_tag_with_refs, *coin.id.object_id(), coin.value())
        }
        ObjectContents::Raw(bytes) => {
            move_value_info_from_raw_bytes(context, &obj_val.type_, bytes)
        }
    }
}

/// Creates `ExtMoveValueInfo` for a coin.
fn coin_move_value_info(
    type_tag_with_refs: TypeTagWithRefs,
    object_id: ObjectID,
    balance: u64,
) -> Result<ExtMoveValueInfo, ExecutionError> {
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
    Ok(ExtMoveValueInfo {
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
