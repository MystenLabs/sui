// Not a license :)

use std::{collections::HashMap, mem::size_of_val, sync::Arc};

use chrono::{DateTime, Utc};
use fastcrypto::encoding::{Base58, Encoding, Hex};
use itertools::Itertools;
use mamoru_sniffer::core::BlockchainData;
use mamoru_sniffer::{
    core::{BlockchainDataBuilder, StructValue, Value, ValueData},
    Sniffer, SnifferConfig,
};
use mamoru_sui_types::{
    CallTrace, CallTraceArg, CallTraceTypeArg, CreatedObject, DeletedObject, Event as MamoruEvent,
    MutatedObject, ObjectOwner, ObjectOwnerKind, ProgrammableTransactionCommand,
    ProgrammableTransactionPublishCommand, ProgrammableTransactionPublishCommandDependency,
    ProgrammableTransactionPublishCommandModule, ProgrammableTransactionUpgradeCommand,
    ProgrammableTransactionUpgradeCommandDependency, ProgrammableTransactionUpgradeCommandModule,
    SuiCtx, Transaction, UnwrappedObject, UnwrappedThenDeletedObject, WrappedObject,
};
use tokio::time::Instant;
use tracing::{info, span, warn, Level};

pub use error::*;
use move_core_types::{
    annotated_value::{MoveStruct, MoveValue},
    trace::{CallTrace as MoveCallTrace, CallType as MoveCallType},
};
use sui_types::base_types::ObjectRef;
use sui_types::inner_temporary_store::InnerTemporaryStore;
use sui_types::object::{Data, Owner};
use sui_types::storage::ObjectStore;
use sui_types::type_resolver::LayoutResolver;
use sui_types::{
    effects::{TransactionEffects, TransactionEffectsAPI},
    event::Event,
    executable_transaction::VerifiedExecutableTransaction,
    transaction::{Command, ProgrammableTransaction, TransactionDataAPI, TransactionKind},
};

mod error;

pub struct SuiSniffer {
    inner: Sniffer,
}

impl SuiSniffer {
    pub async fn new() -> Result<Self, SuiSnifferError> {
        let sniffer =
            Sniffer::new(SnifferConfig::from_env().expect("Missing environment variables")).await?;

        Ok(Self { inner: sniffer })
    }

    pub fn prepare_ctx(
        &self,
        certificate: VerifiedExecutableTransaction,
        effects: TransactionEffects,
        inner_temporary_store: &InnerTemporaryStore,
        call_traces: Vec<MoveCallTrace>,
        time: DateTime<Utc>,
        emit_debug_info: bool,
        layout_resolver: &mut dyn LayoutResolver,
    ) -> Result<BlockchainData<SuiCtx>, SuiSnifferError> {
        if emit_debug_info {
            emit_debug_stats(&call_traces);
        }

        // Sui doesn't have a concept of a transaction sequence number, so we use the current
        // time in nanoseconds.
        let seq = time.timestamp_nanos_opt().unwrap_or_default() as u64;

        let tx_data = certificate.data().transaction_data();
        let tx_hash = format_tx_digest(effects.transaction_digest());
        let call_traces_len = call_traces.len();
        let events = &inner_temporary_store.events.data;
        let events_len = events.len();

        let span = span!(Level::DEBUG, "ctx_builder", ?tx_hash);
        let _guard = span.enter();

        let mut ctx_builder = BlockchainDataBuilder::<SuiCtx>::new();
        ctx_builder.set_tx_data(format!("{}", seq), tx_hash.clone());

        let gas_cost_summary = effects.gas_cost_summary();

        ctx_builder.data_mut().set_tx(Transaction {
            seq,
            digest: tx_hash,
            time: time.timestamp(),
            gas_used: gas_cost_summary.gas_used(),
            gas_computation_cost: gas_cost_summary.computation_cost,
            gas_storage_cost: gas_cost_summary.storage_cost,
            gas_budget: tx_data.gas_budget(),
            sender: format_object_id(certificate.sender_address()),
            kind: tx_data.kind().to_string(),
            success: effects.status().is_ok(),
        });

        let events_timer = Instant::now();
        register_events(ctx_builder.data_mut(), layout_resolver, seq, events);

        info!(
            duration_ms = events_timer.elapsed().as_millis(),
            "sniffer.register_events() executed",
        );

        let call_traces_timer = Instant::now();
        register_call_traces(ctx_builder.data_mut(), seq, call_traces.clone());

        info!(
            duration_ms = call_traces_timer.elapsed().as_millis(),
            "sniffer.register_call_traces() executed",
        );

        let object_changes_timer = Instant::now();
        register_object_changes(
            ctx_builder.data_mut(),
            layout_resolver,
            &effects,
            inner_temporary_store,
        );

        info!(
            duration_ms = object_changes_timer.elapsed().as_millis(),
            "sniffer.register_object_changes() executed",
        );

        if let TransactionKind::ProgrammableTransaction(programmable_tx) = &tx_data.kind() {
            register_programmable_transaction(ctx_builder.data_mut(), programmable_tx);
        }

        ctx_builder.set_statistics(0, 1, events_len as u64, call_traces_len as u64);

        let ctx = ctx_builder.build()?;

        Ok(ctx)
    }

    pub async fn observe_data(&self, data: BlockchainData<SuiCtx>) {
        self.inner.observe_data(data).await;
    }
}

fn register_programmable_transaction(ctx: &mut SuiCtx, tx: &ProgrammableTransaction) {
    let mut publish_command_seq = 0u64;
    let mut publish_command_module_seq = 0u64;
    let mut publish_command_dependency_seq = 0u64;

    let mut upgrade_command_seq = 0u64;
    let mut upgrade_command_module_seq = 0u64;
    let mut upgrade_command_dependency_seq = 0u64;

    for (seq, command) in tx.commands.iter().enumerate() {
        let kind: &'static str = command.into();

        ctx.programmable_transaction_commands
            .push(ProgrammableTransactionCommand {
                seq: seq as u64,
                kind: kind.to_owned(),
            });

        match command {
            Command::Publish(modules, dependencies) => {
                ctx.publish_commands
                    .push(ProgrammableTransactionPublishCommand {
                        seq: publish_command_seq,
                        command_seq: seq as u64,
                    });

                for module in modules {
                    ctx.publish_command_modules
                        .push(ProgrammableTransactionPublishCommandModule {
                            seq: publish_command_module_seq,
                            publish_seq: publish_command_seq,
                            contents: module.clone(),
                        });

                    publish_command_module_seq += 1;
                }

                for dependency in dependencies {
                    ctx.publish_command_dependencies.push(
                        ProgrammableTransactionPublishCommandDependency {
                            seq: publish_command_dependency_seq,
                            publish_seq: publish_command_seq,
                            object_id: format_object_id(dependency),
                        },
                    );

                    publish_command_dependency_seq += 1;
                }

                publish_command_seq += 1;
            }
            Command::Upgrade(modules, dependencies, package_id, _) => {
                ctx.upgrade_commands
                    .push(ProgrammableTransactionUpgradeCommand {
                        seq: upgrade_command_seq,
                        command_seq: seq as u64,
                        package_id: format_object_id(package_id),
                    });

                for module in modules {
                    ctx.upgrade_command_modules
                        .push(ProgrammableTransactionUpgradeCommandModule {
                            seq: upgrade_command_module_seq,
                            upgrade_seq: upgrade_command_seq,
                            contents: module.clone(),
                        });

                    upgrade_command_module_seq += 1;
                }

                for dependency in dependencies {
                    ctx.upgrade_command_dependencies.push(
                        ProgrammableTransactionUpgradeCommandDependency {
                            seq: upgrade_command_dependency_seq,
                            upgrade_seq: upgrade_command_seq,
                            object_id: format_object_id(dependency),
                        },
                    );

                    upgrade_command_dependency_seq += 1;
                }

                upgrade_command_seq += 1;
            }
            _ => continue,
        }
    }
}

fn register_call_traces(ctx: &mut SuiCtx, tx_seq: u64, move_call_traces: Vec<MoveCallTrace>) {
    let mut call_trace_args_len = ctx.call_trace_args.len();
    let mut call_trace_type_args_len = ctx.call_trace_type_args.len();

    let (call_traces, (args, type_args)): (Vec<_>, (Vec<_>, Vec<_>)) = move_call_traces
        .into_iter()
        .zip(0..)
        .map(|(trace, trace_seq)| {
            let trace_seq = trace_seq as u64;

            let call_trace = CallTrace {
                seq: trace_seq,
                tx_seq,
                depth: trace.depth,
                call_type: match trace.call_type {
                    MoveCallType::Call => 0,
                    MoveCallType::CallGeneric => 1,
                },
                gas_used: trace.gas_used,
                transaction_module: trace.module_id.map(|module| module.short_str_lossless()),
                function: trace.function.to_string(),
            };

            let mut cta = vec![];
            let mut ca = vec![];

            for (arg, seq) in trace
                .ty_args
                .into_iter()
                .zip(call_trace_type_args_len as u64..)
            {
                cta.push(CallTraceTypeArg {
                    seq,
                    call_trace_seq: trace_seq,
                    arg: arg.to_canonical_string(true),
                });
            }

            call_trace_type_args_len += cta.len();

            for (arg, seq) in trace.args.into_iter().zip(call_trace_args_len as u64..) {
                match ValueData::new(to_value(&arg)) {
                    Some(arg) => {
                        ca.push(CallTraceArg {
                            seq,
                            call_trace_seq: trace_seq,
                            arg,
                        });
                    }
                    None => continue,
                }
            }

            call_trace_args_len += ca.len();

            (call_trace, (ca, cta))
        })
        .unzip();

    ctx.call_traces.extend(call_traces);
    ctx.call_trace_args.extend(args.into_iter().flatten());
    ctx.call_trace_type_args
        .extend(type_args.into_iter().flatten());
}

fn register_events(
    data: &mut SuiCtx,
    layout_resolver: &mut dyn LayoutResolver,
    tx_seq: u64,
    events: &[Event],
) {
    let mamoru_events: Vec<_> = events
        .iter()
        .filter_map(|event| {
            let Ok(event_struct_layout) = layout_resolver.get_annotated_layout(&event.type_) else {
                warn!(%event.type_, "Can't fetch layout by type");
                return None;
            };

            let Ok(event_struct) =
                Event::move_event_to_move_struct(&event.contents, event_struct_layout)
            else {
                warn!(%event.type_, "Can't parse event contents");
                return None;
            };

            let Some(contents) = ValueData::new(to_value(&MoveValue::Struct(event_struct))) else {
                warn!(%event.type_, "Can't convert event contents to ValueData");
                return None;
            };

            Some(MamoruEvent {
                tx_seq,
                package_id: format_object_id(event.package_id),
                transaction_module: event.transaction_module.clone().into_string(),
                sender: format_object_id(event.sender),
                typ: event.type_.to_canonical_string(true),
                contents,
            })
        })
        .collect();

    data.events.extend(mamoru_events);
}

fn register_object_changes(
    data: &mut SuiCtx,
    layout_resolver: &mut dyn LayoutResolver,
    effects: &TransactionEffects,
    inner_temporary_store: &InnerTemporaryStore,
) {
    let written = &inner_temporary_store.written;

    let mut fetch_move_value = |object_ref: &ObjectRef| {
        let object_id = object_ref.0;

        match written.get_object(&object_id) {
            Ok(Some(object)) => {
                if let Data::Move(move_object) = &object.as_inner().data {
                    let struct_tag = move_object.type_().clone().into();
                    let Ok(layout) = layout_resolver.get_annotated_layout(&struct_tag) else {
                        warn!(%object_id, "Can't fetch layout by struct tag");
                        return None;
                    };

                    let Ok(move_value) = move_object.to_move_struct(&layout) else {
                        warn!(%object_id, "Can't convert to move value");
                        return None;
                    };

                    return Some((object, MoveValue::Struct(move_value)));
                }

                None
            }
            Ok(None) => {
                warn!(%object_id, "Can't fetch object by object id");

                None
            }
            Err(err) => {
                warn!(%err, "Can't fetch object by object id, error");

                None
            }
        }
    };

    let mut object_owner_seq = 0u64;

    for (seq, (created, owner)) in effects.created().iter().enumerate() {
        if let Some((object, move_value)) = fetch_move_value(created) {
            let Some(object_data) = ValueData::new(to_value(&move_value)) else {
                warn!("Can't make ValueData from move value");
                continue;
            };

            data.object_changes.created.push(CreatedObject {
                seq: seq as u64,
                owner_seq: object_owner_seq,
                id: format_object_id(object.id()),
                data: object_data,
            });

            data.object_changes
                .owners
                .push(sui_owner_to_mamoru(object_owner_seq, *owner));
            object_owner_seq += 1;
        }
    }

    for (seq, (mutated, owner)) in effects.mutated().iter().enumerate() {
        if let Some((object, move_value)) = fetch_move_value(mutated) {
            let Some(object_data) = ValueData::new(to_value(&move_value)) else {
                warn!("Can't make ValueData from move value");
                continue;
            };

            data.object_changes.mutated.push(MutatedObject {
                seq: seq as u64,
                owner_seq: object_owner_seq,
                id: format_object_id(object.id()),
                data: object_data,
            });

            data.object_changes
                .owners
                .push(sui_owner_to_mamoru(object_owner_seq, *owner));
            object_owner_seq += 1;
        }
    }

    for (seq, deleted) in effects.deleted().iter().enumerate() {
        data.object_changes.deleted.push(DeletedObject {
            seq: seq as u64,
            id: format_object_id(deleted.0),
        });
    }

    for (seq, wrapped) in effects.wrapped().iter().enumerate() {
        data.object_changes.wrapped.push(WrappedObject {
            seq: seq as u64,
            id: format_object_id(wrapped.0),
        });
    }

    for (seq, (unwrapped, _)) in effects.unwrapped().iter().enumerate() {
        data.object_changes.unwrapped.push(UnwrappedObject {
            seq: seq as u64,
            id: format_object_id(unwrapped.0),
        });
    }

    for (seq, unwrapped_then_deleted) in effects.unwrapped_then_deleted().iter().enumerate() {
        data.object_changes
            .unwrapped_then_deleted
            .push(UnwrappedThenDeletedObject {
                seq: seq as u64,
                id: format_object_id(unwrapped_then_deleted.0),
            });
    }
}

fn sui_owner_to_mamoru(seq: u64, owner: Owner) -> ObjectOwner {
    match owner {
        Owner::AddressOwner(address) => ObjectOwner {
            seq,
            owner_kind: ObjectOwnerKind::Address as u32,
            owner_address: Some(format_object_id(address)),
            initial_shared_version: None,
        },
        Owner::ObjectOwner(address) => ObjectOwner {
            seq,
            owner_kind: ObjectOwnerKind::Object as u32,
            owner_address: Some(format_object_id(address)),
            initial_shared_version: None,
        },
        Owner::Shared {
            initial_shared_version,
        } => ObjectOwner {
            seq,
            owner_kind: ObjectOwnerKind::Shared as u32,
            owner_address: None,
            initial_shared_version: Some(initial_shared_version.into()),
        },
        Owner::Immutable => ObjectOwner {
            seq,
            owner_kind: ObjectOwnerKind::Immutable as u32,
            owner_address: None,
            initial_shared_version: None,
        },
    }
}

fn format_object_id<T: AsRef<[u8]>>(data: T) -> String {
    format!("0x{}", Hex::encode(data))
}

fn format_tx_digest<T: AsRef<[u8]>>(data: T) -> String {
    Base58::encode(data.as_ref())
}

fn to_value(data: &MoveValue) -> Value {
    match data {
        MoveValue::Bool(value) => Value::Bool(*value),
        MoveValue::U8(value) => Value::U64(*value as u64),
        MoveValue::U16(value) => Value::U64(*value as u64),
        MoveValue::U32(value) => Value::U64(*value as u64),
        MoveValue::U64(value) => Value::U64(*value),
        MoveValue::U128(value) => Value::String(format!("{:#x}", value)),
        MoveValue::U256(value) => Value::String(format!("{:#x}", value)),
        MoveValue::Address(addr) | MoveValue::Signer(addr) => Value::String(format_object_id(addr)),
        MoveValue::Vector(value) => Value::List(value.iter().map(to_value).collect()),
        MoveValue::Struct(value) => {
            let MoveStruct { type_, fields } = value;
            let struct_value = StructValue::new(
                type_.to_canonical_string(true),
                fields
                    .iter()
                    .map(|(field, value)| (field.clone().into_string(), to_value(value)))
                    .collect(),
            );

            Value::Struct(struct_value)
        }
    }
}

fn emit_debug_stats(call_traces: &[MoveCallTrace]) {
    let cache_hits_count: usize = call_traces
        .iter()
        .map(|trace| {
            trace
                .args
                .iter()
                // If arc has copies, it's one cache hit.
                .map(|a| if Arc::strong_count(a) > 1 { 1 } else { 0 })
                .sum::<usize>()
        })
        .sum();

    let total_size: usize = call_traces
        .iter()
        .map(|trace| trace.args.iter().map(|a| move_value_size(a)).sum::<usize>())
        .sum();

    let total_call_traces = call_traces.len();

    let top_sized_traces = call_traces
        .iter()
        .map(|trace| trace.args.iter().map(|a| move_value_size(a)).sum::<usize>())
        .collect::<Vec<_>>()
        .into_iter()
        .sorted()
        .rev()
        .take(50)
        .map(bytes_to_human_readable)
        .collect::<Vec<_>>();

    let mut function_call_frequency: HashMap<String, usize> = HashMap::new();

    for trace in call_traces {
        let function = trace
            .module_id
            .as_ref()
            .map(|module| format!("{}::{}", module, &trace.function));

        if let Some(function) = function {
            let count = function_call_frequency.entry(function.clone()).or_insert(0);
            *count += 1;
        }
    }

    let mut most_frequent_calls: Vec<(_, _)> = function_call_frequency.into_iter().collect();

    most_frequent_calls.sort_by(|(_, a), (_, b)| b.cmp(a));
    most_frequent_calls.truncate(50);

    info!(
        total_call_traces = total_call_traces,
        cache_hits_count = %cache_hits_count,
        top_sized_traces = ?top_sized_traces,
        most_frequent_calls = ?most_frequent_calls,
        total_size = bytes_to_human_readable(total_size),
        "call traces debug info"
    );
}

fn move_value_size(value: &MoveValue) -> usize {
    let internal_value_size = match value {
        MoveValue::U8(value) => size_of_val(value),
        MoveValue::U64(value) => size_of_val(value),
        MoveValue::U128(value) => size_of_val(value),
        MoveValue::Bool(value) => size_of_val(value),
        MoveValue::Address(value) => size_of_val(value),
        MoveValue::Vector(value) => value.iter().map(move_value_size).sum::<usize>(),
        MoveValue::Struct(MoveStruct { type_, fields }) => {
            size_of_val(type_)
                + fields
                    .iter()
                    .map(|(a, b)| size_of_val(a) + move_value_size(b))
                    .sum::<usize>()
        }
        MoveValue::Signer(value) => size_of_val(value),
        MoveValue::U16(value) => size_of_val(value),
        MoveValue::U32(value) => size_of_val(value),
        MoveValue::U256(value) => size_of_val(value),
    };

    internal_value_size + std::mem::size_of::<MoveValue>()
}

fn bytes_to_human_readable(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = KB * 1024;

    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_to_string() {
        let command = Command::Publish(vec![], vec![]);
        let value: &'static str = (&command).into();

        assert_eq!(value, String::from("Publish"));
    }
}
