// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_mode::ExecutionMode,
    gas_charger::GasCharger,
    object_runtime, sp,
    static_programmable_transactions::{
        env::Env,
        execution::{
            context::{Context, CtxValue, GasCoinTransfer},
            trace_utils,
        },
        typing::{ast as T, verify::input_arguments::is_coin_send_funds},
    },
};
use move_core_types::account_address::AccountAddress;
use move_trace_format::format::MoveTraceBuilder;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use sui_types::{
    base_types::TxContext,
    error::ExecutionError,
    execution::{ExecutionTiming, ResultWithTimings},
    execution_status::{ExecutionErrorKind, PackageUpgradeError},
    metrics::LimitsMetrics,
    move_package::MovePackage,
    object::Owner,
};
use tracing::instrument;

pub fn execute<'env, 'pc, 'vm, 'state, 'linkage, 'extension, Mode: ExecutionMode>(
    env: &'env mut Env<'pc, 'vm, 'state, 'linkage, 'extension>,
    metrics: Arc<LimitsMetrics>,
    tx_context: Rc<RefCell<TxContext>>,
    gas_charger: &mut GasCharger,
    ast: T::Transaction,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> ResultWithTimings<Mode::ExecutionResults, ExecutionError>
where
    'pc: 'state,
    'env: 'state,
{
    let original_command_len = ast.original_command_len;
    let mut indexed_timings = IndexedExecutionTimings::new(original_command_len);
    let result = execute_inner::<Mode>(
        &mut indexed_timings,
        env,
        metrics,
        tx_context,
        gas_charger,
        ast,
        trace_builder_opt,
    );
    let timings = indexed_timings.into_coalesced();
    debug_assert!(
        timings.len() <= original_command_len,
        "coalesced timings length {} exceeds original command length {}",
        timings.len(),
        original_command_len
    );

    match result {
        Ok(result) => Ok((result, timings)),
        Err(e) => {
            trace_utils::trace_execution_error(trace_builder_opt, e.to_string());
            Err((e, timings))
        }
    }
}

fn execute_inner<'env, 'pc, 'vm, 'state, 'linkage, 'extension, Mode: ExecutionMode>(
    timings: &mut IndexedExecutionTimings,
    env: &'env mut Env<'pc, 'vm, 'state, 'linkage, 'extension>,
    metrics: Arc<LimitsMetrics>,
    tx_context: Rc<RefCell<TxContext>>,
    gas_charger: &mut GasCharger,
    ast: T::Transaction,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> Result<Mode::ExecutionResults, ExecutionError>
where
    'pc: 'state,
{
    debug_assert_eq!(gas_charger.move_gas_status().stack_height_current(), 0);
    let T::Transaction {
        gas_payment,
        bytes,
        objects,
        withdrawals,
        pure,
        receiving,
        withdrawal_compatibility_conversions: _,
        original_command_len: _,
        commands,
    } = ast;
    let mut context = Context::new(
        env,
        metrics,
        tx_context,
        gas_charger,
        gas_payment,
        bytes,
        objects,
        withdrawals,
        pure,
        receiving,
    )?;

    trace_utils::trace_ptb_summary(&mut context, trace_builder_opt, &commands)?;

    let mut mode_results = Mode::empty_results();
    for sp!(annotated_index, c) in commands {
        let annotated_index = annotated_index as usize;
        let start = Instant::now();
        if let Err(err) =
            execute_command::<Mode>(&mut context, &mut mode_results, c, trace_builder_opt)
        {
            // We still need to record the loaded child objects for replay
            let loaded_runtime_objects = object_runtime!(context)?.loaded_runtime_objects();
            // we do not save the wrapped objects since on error, they should not be modified
            drop(context);
            // TODO wtf is going on with the borrow checker here. 'state is bound into the object
            // runtime, but its since been dropped. what gives with this error?
            env.state_view
                .save_loaded_runtime_objects(loaded_runtime_objects);
            timings.error(annotated_index, start.elapsed());
            return Err(err.with_command_index(annotated_index));
        };
        timings.executed(annotated_index, start.elapsed());
    }
    // Save loaded objects table in case we fail in post execution
    //
    // We still need to record the loaded child objects for replay
    // Record the objects loaded at runtime (dynamic fields + received) for
    // storage rebate calculation.
    let loaded_runtime_objects = object_runtime!(context)?.loaded_runtime_objects();
    // We record what objects were contained in at the start of the transaction
    // for expensive invariant checks
    let wrapped_object_containers = object_runtime!(context)?.wrapped_object_containers();
    // We record the generated object IDs for expensive invariant checks
    let generated_object_ids = object_runtime!(context)?.generated_object_ids();

    // apply changes
    let finished = context.finish::<Mode>();
    // Save loaded objects for debug. We dont want to lose the info
    env.state_view
        .save_loaded_runtime_objects(loaded_runtime_objects);
    env.state_view
        .save_wrapped_object_containers(wrapped_object_containers);
    env.state_view.record_execution_results(finished?)?;
    env.state_view
        .record_generated_object_ids(generated_object_ids);
    Ok(mode_results)
}

/// Execute a single command
#[instrument(level = "trace", skip_all)]
fn execute_command<Mode: ExecutionMode>(
    context: &mut Context,
    mode_results: &mut Mode::ExecutionResults,
    c: T::Command_,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> Result<(), ExecutionError> {
    let T::Command_ {
        command,
        result_type,
        drop_values,
        consumed_shared_objects: _,
    } = c;
    assert_invariant!(
        context.gas_charger.move_gas_status().stack_height_current() == 0,
        "stack height did not start at 0"
    );
    let is_move_call = matches!(command, T::Command__::MoveCall(_));
    let num_args = command.arguments_len();
    let mut args_to_update = vec![];
    let result = match command {
        T::Command__::MoveCall(move_call) => {
            trace_utils::trace_move_call_start(trace_builder_opt);
            let T::MoveCall {
                function,
                arguments,
            } = *move_call;
            // Detect send_funds with gas coin
            let is_gas_coin_send_funds = is_coin_send_funds(&function)
                && arguments.first().is_some_and(|arg| {
                    matches!(
                        &arg.value.0,
                        T::Argument__::Use(T::Usage::Move(T::Location::GasCoin))
                    )
                });
            if Mode::TRACK_EXECUTION {
                args_to_update.extend(
                    arguments
                        .iter()
                        .filter(|arg| matches!(&arg.value.1, T::Type::Reference(/* mut */ true, _)))
                        .cloned(),
                )
            }
            let arguments: Vec<CtxValue> = context.arguments(arguments)?;
            if is_gas_coin_send_funds {
                assert_invariant!(arguments.len() == 2, "coin::send_funds should have 2 args");
                let recipient = arguments.last().unwrap().to_address()?;
                context.record_gas_coin_transfer(GasCoinTransfer::SendFunds { recipient })?;
            }
            let res = context.vm_move_call(function, arguments, trace_builder_opt);
            trace_utils::trace_move_call_end(trace_builder_opt);
            res?
        }
        T::Command__::TransferObjects(objects, recipient) => {
            // Check if any object is the gas coin moved by value before consuming
            let has_gas_coin_move = objects.iter().any(|arg| {
                matches!(
                    &arg.value.0,
                    T::Argument__::Use(T::Usage::Move(T::Location::GasCoin))
                )
            });
            if has_gas_coin_move {
                context.record_gas_coin_transfer(GasCoinTransfer::TransferObjects)?;
            }
            let object_tys = objects
                .iter()
                .map(|sp!(_, (_, ty))| ty.clone())
                .collect::<Vec<_>>();
            let object_values: Vec<CtxValue> = context.arguments(objects)?;
            let recipient: AccountAddress = context.argument(recipient)?;
            assert_invariant!(
                object_values.len() == object_tys.len(),
                "object values and types mismatch"
            );
            trace_utils::trace_transfer(context, trace_builder_opt, &object_values, &object_tys)?;
            for (object_value, ty) in object_values.into_iter().zip(object_tys) {
                // TODO should we just call a Move function?
                let recipient = Owner::AddressOwner(recipient.into());
                context.transfer_object(recipient, ty, object_value)?;
            }
            vec![]
        }
        T::Command__::SplitCoins(ty, coin, amounts) => {
            let mut trace_values = vec![];
            // TODO should we just call a Move function?
            if Mode::TRACK_EXECUTION {
                args_to_update.push(coin.clone());
            }
            let coin_ref: CtxValue = context.argument(coin)?;
            let amount_values: Vec<u64> = context.arguments(amounts)?;
            let mut total: u64 = 0;
            for amount in &amount_values {
                let Some(new_total) = total.checked_add(*amount) else {
                    return Err(ExecutionError::from_kind(
                        ExecutionErrorKind::CoinBalanceOverflow,
                    ));
                };
                total = new_total;
            }
            trace_utils::add_move_value_info_from_ctx_value(
                context,
                trace_builder_opt,
                &mut trace_values,
                &ty,
                &coin_ref,
            )?;
            let coin_value = context.copy_value(&coin_ref)?.coin_ref_value()?;
            fp_ensure!(
                coin_value >= total,
                ExecutionError::new_with_source(
                    ExecutionErrorKind::InsufficientCoinBalance,
                    format!("balance: {coin_value} required: {total}")
                )
            );
            coin_ref.coin_ref_subtract_balance(total)?;
            let amounts = amount_values
                .into_iter()
                .map(|a| context.new_coin(a))
                .collect::<Result<Vec<_>, _>>()?;
            trace_utils::trace_split_coins(
                context,
                trace_builder_opt,
                &ty,
                trace_values,
                &amounts,
                total,
            )?;

            amounts
        }
        T::Command__::MergeCoins(ty, target, coins) => {
            let mut trace_values = vec![];
            // TODO should we just call a Move function?
            if Mode::TRACK_EXECUTION {
                args_to_update.push(target.clone());
            }
            let target_ref: CtxValue = context.argument(target)?;
            trace_utils::add_move_value_info_from_ctx_value(
                context,
                trace_builder_opt,
                &mut trace_values,
                &ty,
                &target_ref,
            )?;
            let coins = context.arguments(coins)?;
            let amounts = coins
                .into_iter()
                .map(|coin| {
                    trace_utils::add_move_value_info_from_ctx_value(
                        context,
                        trace_builder_opt,
                        &mut trace_values,
                        &ty,
                        &coin,
                    )?;
                    context.destroy_coin(coin)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut additional: u64 = 0;
            for amount in amounts {
                let Some(new_additional) = additional.checked_add(amount) else {
                    return Err(ExecutionError::from_kind(
                        ExecutionErrorKind::CoinBalanceOverflow,
                    ));
                };
                additional = new_additional;
            }
            let target_value = context.copy_value(&target_ref)?.coin_ref_value()?;
            fp_ensure!(
                target_value.checked_add(additional).is_some(),
                ExecutionError::from_kind(ExecutionErrorKind::CoinBalanceOverflow,)
            );
            target_ref.coin_ref_add_balance(additional)?;
            trace_utils::trace_merge_coins(
                context,
                trace_builder_opt,
                &ty,
                trace_values,
                additional,
            )?;
            vec![]
        }
        T::Command__::MakeMoveVec(ty, items) => {
            let items: Vec<CtxValue> = context.arguments(items)?;
            trace_utils::trace_make_move_vec(context, trace_builder_opt, &items, &ty)?;
            vec![CtxValue::vec_pack(ty, items)?]
        }
        T::Command__::Publish(module_bytes, dep_ids, linkage) => {
            trace_utils::trace_publish_event(trace_builder_opt)?;
            let modules =
                context.deserialize_modules(&module_bytes, /* is upgrade */ false)?;

            let original_id = context.publish_and_init_package::<Mode>(
                modules,
                &dep_ids,
                linkage,
                trace_builder_opt,
            )?;

            if <Mode>::packages_are_predefined() {
                // no upgrade cap for genesis modules
                std::vec![]
            } else {
                std::vec![context.new_upgrade_cap(original_id)?]
            }
        }
        T::Command__::Upgrade(
            module_bytes,
            dep_ids,
            current_package_id,
            upgrade_ticket,
            linkage,
        ) => {
            trace_utils::trace_upgrade_event(trace_builder_opt)?;
            let upgrade_ticket = context
                .argument::<CtxValue>(upgrade_ticket)?
                .into_upgrade_ticket()?;
            // Make sure the passed-in package ID matches the package ID in the `upgrade_ticket`.
            if current_package_id != upgrade_ticket.package.bytes {
                return Err(ExecutionError::from_kind(
                    ExecutionErrorKind::PackageUpgradeError {
                        upgrade_error: PackageUpgradeError::PackageIDDoesNotMatch {
                            package_id: current_package_id,
                            ticket_id: upgrade_ticket.package.bytes,
                        },
                    },
                ));
            }
            // deserialize modules and charge gas
            let modules = context.deserialize_modules(&module_bytes, /* is upgrade */ true)?;

            let computed_digest = MovePackage::compute_digest_for_modules_and_deps(
                &module_bytes,
                &dep_ids,
                /* hash_modules */ true,
            )
            .to_vec();
            if computed_digest != upgrade_ticket.digest {
                return Err(ExecutionError::from_kind(
                    ExecutionErrorKind::PackageUpgradeError {
                        upgrade_error: PackageUpgradeError::DigestDoesNotMatch {
                            digest: computed_digest,
                        },
                    },
                ));
            }

            let upgraded_package_id = context.upgrade(
                modules,
                &dep_ids,
                current_package_id,
                upgrade_ticket.policy,
                linkage,
            )?;

            vec![context.upgrade_receipt(upgrade_ticket, upgraded_package_id)]
        }
    };
    if Mode::TRACK_EXECUTION {
        let argument_updates = context.argument_updates(args_to_update)?;
        let command_result = context.tracked_results(&result, &result_type)?;
        Mode::finish_command_v2(mode_results, argument_updates, command_result)?;
    }
    assert_invariant!(
        result.len() == drop_values.len(),
        "result values and drop values mismatch"
    );
    context.charge_command(is_move_call, num_args, result.len())?;
    let result = result
        .into_iter()
        .zip(drop_values)
        .map(|(value, drop)| if !drop { Some(value) } else { None })
        .collect::<Vec<_>>();
    context.result(result)?;
    assert_invariant!(
        context.gas_charger.move_gas_status().stack_height_current() == 0,
        "stack height did not end at 0"
    );
    Ok(())
}

/// Struct to track execution timings, coalesced into the annotated command indices.
struct IndexedExecutionTimings {
    /// The maximum index in the original command vector. All annotated indices will be capped at
    /// this value.
    max_allowed_index: usize,
    /// Mapping from the command's annotated index to its duration. Multiple commands may share
    /// the same annotated index, in which case their durations will be added together.
    executed_commands: BTreeMap<usize, Duration>,
    /// `Some` if an error occurred, stopping execution.
    /// `usize` is the annotated index of the command.
    error_command: Option<(usize, Duration)>,
}

impl IndexedExecutionTimings {
    fn new(original_command_len: usize) -> Self {
        let max_allowed_index = original_command_len.saturating_sub(1);
        Self {
            max_allowed_index,
            executed_commands: BTreeMap::new(),
            error_command: None,
        }
    }

    /// Records the execution of a successful command.
    fn executed(&mut self, annotated_index: usize, duration: Duration) {
        debug_assert!(
            self.error_command.is_none(),
            "command executed after an error occurred"
        );
        let index = annotated_index.min(self.max_allowed_index);
        let existing = self
            .executed_commands
            .entry(index)
            .or_insert(Duration::ZERO);
        *existing = existing.saturating_add(duration);
    }

    /// Record the execution of a failed command that errored and stopped the execution of the PTB.
    fn error(&mut self, annotated_index: usize, duration: Duration) {
        debug_assert!(self.error_command.is_none(), "multiple errors recorded");
        let index = annotated_index.min(self.max_allowed_index);
        debug_assert!(
            self.executed_commands
                .last_key_value()
                .is_none_or(|(last, _)| *last <= index),
            "execution timings recorded for command index {:?} after error at index {}",
            self.executed_commands
                .last_key_value()
                .map(|(last, _)| *last),
            index,
        );

        let existing_opt = self.executed_commands.remove(&index);
        let total_duration = existing_opt
            .unwrap_or(Duration::ZERO)
            .saturating_add(duration);
        self.error_command = Some((index, total_duration));
    }

    /// Coalesces timings by each commands annotated index to align with the original command count.
    /// Extra commands may have been injected during typing (e.g., withdrawal compatibility).
    /// Timings sharing an `annotated_index` have their durations summed. An error, if present,
    /// is always last.
    fn into_coalesced(self) -> Vec<ExecutionTiming> {
        let Self {
            max_allowed_index,
            executed_commands,
            error_command,
        } = self;

        let max_executed_index = executed_commands.keys().last().copied();
        let error_index = error_command.as_ref().map(|(idx, _)| *idx);
        let max_used_index = match (max_executed_index, error_index) {
            (Some(exec), Some(err)) => exec.max(err),
            (Some(idx), None) | (None, Some(idx)) => idx,
            (None, None) => return vec![],
        };
        debug_assert!(
            max_used_index <= max_allowed_index,
            "max used index {} exceeds max allowed index {}",
            max_used_index,
            max_allowed_index
        );
        let size = max_used_index.saturating_add(1);

        // We initialize a vector of `Success` timings with zero duration, since we have no
        // guarantee at this point that there are no gaps in the annotated indices. Presently,
        // there should be no gaps, but there is nothing inherent to the annotation scheme that
        // guarantees they are not sparse.
        let mut coalesced = vec![ExecutionTiming::Success(Duration::ZERO); size];
        for (index, duration) in executed_commands {
            let Some(entry) = coalesced.get_mut(index) else {
                debug_assert!(
                    false,
                    "failed to initialize coalesced timings at index {}",
                    index
                );
                continue;
            };
            debug_assert!(matches!(entry, ExecutionTiming::Success(d) if d.is_zero()));
            *entry = ExecutionTiming::Success(duration);
        }

        if let Some((index, error_duration)) = error_command {
            debug_assert!(
                index == coalesced.len().saturating_sub(1),
                "error index should be last"
            );
            if let Some(entry) = coalesced.get_mut(index) {
                debug_assert!(matches!(entry, ExecutionTiming::Success(d) if d.is_zero()));
                *entry = ExecutionTiming::Abort(error_duration);
            } else {
                debug_assert!(
                    false,
                    "failed to initialize coalesced timings at index {}",
                    index
                );
            };
        }

        coalesced
    }
}
