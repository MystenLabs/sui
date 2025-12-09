// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module implements support for tracing related to PTB execution. IMPORTANT:
//! Bodies of all public functions in this module should be enclosed in a large if statement checking if
//! tracing is enabled or not to make sure that any errors coming from these functions only manifest itself
//! when tracing is enabled.

use crate::static_programmable_transactions::{
    execution::context::{Context, CtxValue},
    typing::ast::{Command__, Commands, Type},
};
use move_core_types::{annotated_value as A, language_storage::TypeTag};
use move_trace_format::{
    format::{Effect, MoveTraceBuilder, RefType, TraceEvent, TypeTagWithRefs},
    value::{SerializableMoveValue, SimplifiedMoveStruct},
};
use move_vm_types::values::Value as VMValue;
use sui_types::{
    error::ExecutionError,
    ptb_trace::{
        ExtMoveValue, ExtMoveValueInfo, ExternalEvent, PTBCommandInfo, PTBEvent, SummaryEvent,
    },
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
    context: &mut Context,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    values: &[CtxValue],
    tys: &[Type],
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        let mut to_transfer = vec![];
        for (idx, (v, ty)) in values.iter().zip(tys).enumerate() {
            let tag = adapter_type_to_type_tag_with_refs(ty)?;
            let layout = annotated_type_layout_for_adapter_ty(context, ty)?;
            let value = serializable_move_value_from_ctx_value(v, &layout)?;
            to_transfer.push(ExtMoveValue::Single {
                name: format!("obj{idx}"),
                info: ExtMoveValueInfo { type_: tag, value },
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
pub fn trace_ptb_summary(
    context: &mut Context,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    commands: &Commands,
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        let events = commands
            .iter()
            .map(|c| match &c.value.command {
                Command__::MoveCall(move_call) => {
                    let pkg = move_call
                        .function
                        .storage_id
                        .address()
                        .to_canonical_string(/*with_prefix*/ true);
                    let module = move_call.function.storage_id.name().to_string();
                    let function = move_call.function.name.to_string();
                    Ok(vec![PTBCommandInfo::MoveCall {
                        pkg,
                        module,
                        function,
                    }])
                }
                Command__::TransferObjects(..) => Ok(vec![PTBCommandInfo::ExternalEvent(
                    "TransferObjects".to_string(),
                )]),
                Command__::SplitCoins(..) => Ok(vec![PTBCommandInfo::ExternalEvent(
                    "SplitCoins".to_string(),
                )]),
                Command__::MergeCoins(..) => Ok(vec![PTBCommandInfo::ExternalEvent(
                    "MergeCoins".to_string(),
                )]),
                Command__::Publish(module_bytes, _, _) => {
                    let mut events = vec![];
                    events.push(PTBCommandInfo::ExternalEvent("Publish".to_string()));
                    // Not ideal but it only runs when tracing is enabled so overhead
                    // should be insignificant
                    let modules = context.deserialize_modules(module_bytes, false)?;
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
                Command__::MakeMoveVec(..) => Ok(vec![PTBCommandInfo::ExternalEvent(
                    "MakeMoveVec".to_string(),
                )]),
                Command__::Upgrade(..) => {
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
    context: &mut Context,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    coin_type: &Type,
    input_coin: Vec<ExtMoveValueInfo>,
    split_coin_values: &[CtxValue],
    total_split_value: u64,
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        let type_tag_with_refs = adapter_type_to_type_tag_with_refs(coin_type)?;
        let layout = annotated_type_layout_for_adapter_ty(context, coin_type)?;
        let mut split_coin_move_values = vec![];
        for coin_val in split_coin_values {
            let coin_val = serializable_move_value_from_ctx_value(coin_val, &layout)?;
            split_coin_move_values.push(coin_val);
        }

        let Some([mut input]): Option<[ExtMoveValueInfo; 1]> = input_coin.try_into().ok() else {
            invariant_violation!("Expected exactly one input coin for tracing `SplitCoins`");
        };

        update_coin_balance(&mut input, |current_balance| {
            current_balance.saturating_sub(total_split_value)
        })?;

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
    _context: &mut Context,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    _coin_type: &Type,
    mut trace_values: Vec<ExtMoveValueInfo>,
    total_merged_value: u64,
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        if trace_values.is_empty() {
            invariant_violation!("Missing destination coin for tracing `MergeCoins`");
        }

        let target_coin_starting_state = trace_values.remove(0);
        let mut target_coin_resulting_state = target_coin_starting_state.clone();
        update_coin_balance(&mut target_coin_resulting_state, |current_balance| {
            current_balance.saturating_add(total_merged_value)
        })?;

        let mut values = vec![ExtMoveValue::Single {
            name: "merge_target".to_string(),
            info: target_coin_starting_state,
        }];
        for (idx, input_value) in trace_values.into_iter().enumerate() {
            values.push(ExtMoveValue::Single {
                name: format!("coin{idx}"),
                info: input_value,
            });
        }
        values.push(ExtMoveValue::Single {
            name: "merge_result".to_string(),
            info: target_coin_resulting_state,
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
    context: &mut Context,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    values: &[CtxValue],
    type_: &Type,
) -> Result<(), ExecutionError> {
    if let Some(trace_builder) = trace_builder_opt {
        let type_tag_with_refs = adapter_type_to_type_tag_with_refs(type_)?;
        let layout = annotated_type_layout_for_adapter_ty(context, type_)?;
        let values: Vec<SerializableMoveValue> = values
            .iter()
            .map(|ctx_value| serializable_move_value_from_ctx_value(ctx_value, &layout))
            .collect::<Result<_, _>>()?;
        trace_builder.push_event(TraceEvent::External(Box::new(serde_json::json!(
            PTBEvent::ExternalEvent(ExternalEvent {
                description: "MakeMoveVec: vector".to_string(),
                name: "MakeMoveVec".to_string(),
                values: vec![ExtMoveValue::Vector {
                    name: "vector".to_string(),
                    type_: type_tag_with_refs,
                    value: values,
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
pub fn add_move_value_info_from_ctx_value(
    context: &mut Context,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
    move_values: &mut Vec<ExtMoveValueInfo>,
    type_: &Type,
    value: &CtxValue,
) -> Result<(), ExecutionError> {
    if trace_builder_opt.is_some()
        && let Some(move_value_info) = move_value_info_from_ctx_value(context, type_, value)?
    {
        move_values.push(move_value_info);
    }
    Ok(())
}

/// Creates `ExtMoveValueInfo` from `Value`.
fn move_value_info_from_ctx_value(
    context: &mut Context,
    type_: &Type,
    value: &CtxValue,
) -> Result<Option<ExtMoveValueInfo>, ExecutionError> {
    let layout = annotated_type_layout_for_adapter_ty(context, type_)?;
    let value = serializable_move_value_from_ctx_value(value, &layout)?;
    let type_ = adapter_type_to_type_tag_with_refs(type_)?;
    Ok(Some(ExtMoveValueInfo { type_, value }))
}

fn annotated_type_layout_for_adapter_ty(
    context: &mut Context,
    type_: &Type,
) -> Result<A::MoveTypeLayout, ExecutionError> {
    let ty = context
        .env
        .load_vm_type_argument_from_adapter_type(0, type_)?;
    context
        .env
        .vm
        .get_runtime()
        .type_to_fully_annotated_layout(&ty)
        .map_err(|e| {
            make_invariant_violation!(
                "Failed to get annotated type layout for adapter type: {}",
                e
            )
        })
}

/// Creates a `SerializableMoveValue` (a Move value for the trace format) from a `CtxValue` and a
/// provided annotated layout for that value.
fn serializable_move_value_from_ctx_value(
    value: &CtxValue,
    annotated_layout: &A::MoveTypeLayout,
) -> Result<SerializableMoveValue, ExecutionError> {
    VMValue::as_annotated_move_value_for_tracing_only(
        value.inner_for_tracing().inner_for_tracing(),
        annotated_layout,
    )
    .ok_or_else(|| {
        make_invariant_violation!(
            "Failed to convert Move value to `SerializableMoveValue` for tracing"
        )
    })
    .map(SerializableMoveValue::from)
}

fn update_coin_balance(
    coin: &mut ExtMoveValueInfo,
    balance_update: impl Fn(u64) -> u64,
) -> Result<(), ExecutionError> {
    use SerializableMoveValue as SMV;
    let SMV::Struct(SimplifiedMoveStruct { fields, .. }) = &mut coin.value else {
        invariant_violation!("Expected coin to be a struct");
    };

    let [_, (_, balance_value)] = fields.as_mut_slice() else {
        invariant_violation!("Expected coin struct to have two fields");
    };

    let SMV::Struct(SimplifiedMoveStruct { fields, .. }) = balance_value else {
        invariant_violation!("Expected balance field to be a struct");
    };

    let [(_, SMV::U64(current_balance))] = fields.as_mut_slice() else {
        invariant_violation!("Expected balance struct to have a single u64 field");
    };

    *current_balance = balance_update(*current_balance);

    Ok(())
}

/// Converts a type to type tag format used in tracing.
fn adapter_type_to_type_tag_with_refs(type_: &Type) -> Result<TypeTagWithRefs, ExecutionError> {
    Ok(match type_ {
        Type::Reference(mutable, inner_ty) => {
            let ref_type = if *mutable { RefType::Mut } else { RefType::Imm };
            let inner_type: TypeTag = Type::try_into((**inner_ty).clone()).map_err(|e| {
                make_invariant_violation!(
                    "Failed to convert adapter type to type tag for tracing: {}",
                    e
                )
            })?;
            TypeTagWithRefs {
                type_: inner_type,
                ref_type: Some(ref_type),
            }
        }
        ty => {
            let type_: TypeTag = Type::try_into(ty.clone()).map_err(|e| {
                make_invariant_violation!(
                    "Failed to convert adapter type to type tag for tracing: {}",
                    e
                )
            })?;

            TypeTagWithRefs {
                type_,
                ref_type: None,
            }
        }
    })
}
