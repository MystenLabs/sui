// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_mode::ExecutionMode,
    gas_charger::GasCharger,
    sp,
    static_programmable_transactions::{
        env::Env,
        execution::context::{Context, CtxValue},
        typing::ast as T,
    },
};
use move_core_types::account_address::AccountAddress;
use move_trace_format::format::MoveTraceBuilder;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Instant};
use sui_types::{
    base_types::TxContext,
    error::{ExecutionError, ExecutionErrorKind},
    execution::{ExecutionTiming, ResultWithTimings},
    execution_status::PackageUpgradeError,
    metrics::LimitsMetrics,
    move_package::MovePackage,
    object::Owner,
};
use tracing::instrument;

pub fn execute<'env, 'pc, 'vm, 'state, 'linkage, Mode: ExecutionMode>(
    env: &'env mut Env<'pc, 'vm, 'state, 'linkage>,
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
    let mut timings = vec![];
    let result = execute_inner::<Mode>(
        &mut timings,
        env,
        metrics,
        tx_context,
        gas_charger,
        ast,
        trace_builder_opt,
    );

    match result {
        Ok(result) => Ok((result, timings)),
        Err(e) => Err((e, timings)),
    }
}

pub fn execute_inner<'env, 'pc, 'vm, 'state, 'linkage, Mode: ExecutionMode>(
    timings: &mut Vec<ExecutionTiming>,
    env: &'env mut Env<'pc, 'vm, 'state, 'linkage>,
    metrics: Arc<LimitsMetrics>,
    tx_context: Rc<RefCell<TxContext>>,
    gas_charger: &mut GasCharger,
    ast: T::Transaction,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> Result<Mode::ExecutionResults, ExecutionError>
where
    'pc: 'state,
{
    let T::Transaction {
        bytes,
        objects,
        withdrawals,
        pure,
        receiving,
        commands,
    } = ast;
    let mut context = Context::new(
        env,
        metrics,
        tx_context,
        gas_charger,
        bytes,
        objects,
        withdrawals,
        pure,
        receiving,
    )?;
    let mut mode_results = Mode::empty_results();
    for sp!(idx, c) in commands {
        let start = Instant::now();
        if let Err(err) = execute_command::<Mode>(
            &mut context,
            &mut mode_results,
            c,
            trace_builder_opt.as_mut(),
        ) {
            let object_runtime = context.object_runtime()?;
            // We still need to record the loaded child objects for replay
            let loaded_runtime_objects = object_runtime.loaded_runtime_objects();
            // we do not save the wrapped objects since on error, they should not be modified
            drop(context);
            // TODO wtf is going on with the borrow checker here. 'state is bound into the object
            // runtime, but its since been dropped. what gives with this error?
            env.state_view
                .save_loaded_runtime_objects(loaded_runtime_objects);
            timings.push(ExecutionTiming::Abort(start.elapsed()));
            return Err(err.with_command_index(idx as usize));
        };
        timings.push(ExecutionTiming::Success(start.elapsed()));
    }
    // Save loaded objects table in case we fail in post execution
    let object_runtime = context.object_runtime()?;
    // We still need to record the loaded child objects for replay
    // Record the objects loaded at runtime (dynamic fields + received) for
    // storage rebate calculation.
    let loaded_runtime_objects = object_runtime.loaded_runtime_objects();
    // We record what objects were contained in at the start of the transaction
    // for expensive invariant checks
    let wrapped_object_containers = object_runtime.wrapped_object_containers();
    // We record the generated object IDs for expensive invariant checks
    let generated_object_ids = object_runtime.generated_object_ids();

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
    trace_builder_opt: Option<&mut MoveTraceBuilder>,
) -> Result<(), ExecutionError> {
    let T::Command_ {
        command,
        result_type,
        drop_values,
        consumed_shared_objects: _,
    } = c;
    let mut args_to_update = vec![];
    let result = match command {
        T::Command__::MoveCall(move_call) => {
            let T::MoveCall {
                function,
                arguments,
            } = *move_call;
            if Mode::TRACK_EXECUTION {
                args_to_update.extend(
                    arguments
                        .iter()
                        .filter(|arg| matches!(&arg.value.1, T::Type::Reference(/* mut */ true, _)))
                        .cloned(),
                )
            }
            let arguments = context.arguments(arguments)?;
            context.vm_move_call(function, arguments, trace_builder_opt)?
        }
        T::Command__::TransferObjects(objects, recipient) => {
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
            for (object_value, ty) in object_values.into_iter().zip(object_tys) {
                // TODO should we just call a Move function?
                let recipient = Owner::AddressOwner(recipient.into());
                context.transfer_object(recipient, ty, object_value)?;
            }
            vec![]
        }
        T::Command__::SplitCoins(_, coin, amounts) => {
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
            let coin_value = context.copy_value(&coin_ref)?.coin_ref_value()?;
            fp_ensure!(
                coin_value >= total,
                ExecutionError::new_with_source(
                    ExecutionErrorKind::InsufficientCoinBalance,
                    format!("balance: {coin_value} required: {total}")
                )
            );
            coin_ref.coin_ref_subtract_balance(total)?;
            amount_values
                .into_iter()
                .map(|a| context.new_coin(a))
                .collect::<Result<_, _>>()?
        }
        T::Command__::MergeCoins(_, target, coins) => {
            // TODO should we just call a Move function?
            if Mode::TRACK_EXECUTION {
                args_to_update.push(target.clone());
            }
            let target_ref: CtxValue = context.argument(target)?;
            let coins = context.arguments(coins)?;
            let amounts = coins
                .into_iter()
                .map(|coin| context.destroy_coin(coin))
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
            vec![]
        }
        T::Command__::MakeMoveVec(ty, items) => {
            let items: Vec<CtxValue> = context.arguments(items)?;
            vec![CtxValue::vec_pack(ty, items)?]
        }
        T::Command__::Publish(module_bytes, dep_ids, linkage) => {
            let modules =
                context.deserialize_modules(&module_bytes, /* is upgrade */ false)?;

            let runtime_id = context.publish_and_init_package::<Mode>(
                modules,
                &dep_ids,
                linkage,
                trace_builder_opt,
            )?;

            if <Mode>::packages_are_predefined() {
                // no upgrade cap for genesis modules
                std::vec![]
            } else {
                std::vec![context.new_upgrade_cap(runtime_id)?]
            }
        }
        T::Command__::Upgrade(
            module_bytes,
            dep_ids,
            current_package_id,
            upgrade_ticket,
            linkage,
        ) => {
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
    let result = result
        .into_iter()
        .zip(drop_values)
        .map(|(value, drop)| if !drop { Some(value) } else { None })
        .collect::<Vec<_>>();
    context.result(result)?;
    Ok(())
}
