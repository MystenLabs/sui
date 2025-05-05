// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_value::ExecutionState,
    gas_charger::GasCharger,
    static_programmable_transactions::{
        env::Env,
        execution::{context::Context, values},
        typing::ast as T,
    },
};
use move_core_types::account_address::AccountAddress;
use move_trace_format::format::MoveTraceBuilder;
use move_vm_types::values::Value;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Instant};
use sui_types::{
    base_types::TxContext,
    error::{ExecutionError, ExecutionErrorKind},
    execution::{ExecutionTiming, ResultWithTimings},
    metrics::LimitsMetrics,
    object::Owner,
};
use tracing::instrument;

pub fn execute<'env, 'pc, 'vm, 'state, 'linkage>(
    env: &'env mut Env<'pc, 'vm, 'state, 'linkage>,
    metrics: Arc<LimitsMetrics>,
    tx_context: Rc<RefCell<TxContext>>,
    gas_charger: &mut GasCharger,
    ast: T::Transaction,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> ResultWithTimings<(), ExecutionError>
where
    'pc: 'state,
    'env: 'state,
{
    let mut timings = vec![];
    let result = execute_inner(
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

pub fn execute_inner<'env, 'pc, 'vm, 'state, 'linkage>(
    timings: &mut Vec<ExecutionTiming>,
    env: &'env mut Env<'pc, 'vm, 'state, 'linkage>,
    metrics: Arc<LimitsMetrics>,
    tx_context: Rc<RefCell<TxContext>>,
    gas_charger: &mut GasCharger,
    ast: T::Transaction,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> Result<(), ExecutionError>
where
    'pc: 'state,
    'env: 'state,
{
    let state_view: *mut dyn ExecutionState = env.state_view;
    let T::Transaction { inputs, commands } = ast;
    let mut context = Context::new(env, metrics, tx_context, gas_charger, inputs)?;
    for (idx, (command, tys)) in commands.into_iter().enumerate() {
        let start = Instant::now();
        if let Err(err) = execute_command(&mut context, command, tys, trace_builder_opt.as_mut()) {
            let object_runtime = context.object_runtime()?;
            // We still need to record the loaded child objects for replay
            let loaded_runtime_objects = object_runtime.loaded_runtime_objects();
            // we do not save the wrapped objects since on error, they should not be modified
            drop(context);
            // TODO wtf is going on with the borrow checker here. 'state is bound into the object
            // runtime, but its since been dropped. what gives with this error?
            let state_view: &mut dyn ExecutionState = unsafe { state_view.as_mut().unwrap() };
            state_view.save_loaded_runtime_objects(loaded_runtime_objects);
            timings.push(ExecutionTiming::Abort(start.elapsed()));
            return Err(err.with_command_index(idx));
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

    // apply changes
    let finished = context.finish();
    // TODO wtf is going on with the borrow checker here
    let state_view: &mut dyn ExecutionState = unsafe { state_view.as_mut().unwrap() };
    // Save loaded objects for debug. We dont want to lose the info
    state_view.save_loaded_runtime_objects(loaded_runtime_objects);
    state_view.save_wrapped_object_containers(wrapped_object_containers);
    state_view.record_execution_results(finished?);
    Ok(())
}

/// Execute a single command
#[instrument(level = "trace", skip_all)]
fn execute_command(
    context: &mut Context,
    command: T::Command,
    _result_tys: T::ResultType,
    trace_builder_opt: Option<&mut MoveTraceBuilder>,
) -> Result<(), ExecutionError> {
    let result = match command {
        T::Command::MoveCall(move_call) => {
            let T::MoveCall {
                function,
                arguments,
            } = *move_call;
            let arguments = context.arguments(arguments)?;
            context.vm_move_call(function, arguments, trace_builder_opt)?
        }
        T::Command::TransferObjects(objects, recipient) => {
            let object_tys = objects.iter().map(|(_, ty)| ty.clone()).collect::<Vec<_>>();
            let object_values: Vec<Value> = context.arguments(objects)?;
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
        T::Command::SplitCoins(_, coin, amounts) => {
            // TODO should we just call a Move function?
            let coin_ref: Value = context.argument(coin)?;
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
            let coin_value = values::coin_value(context.copy_value(&coin_ref)?)?;
            fp_ensure!(
                coin_value >= total,
                ExecutionError::new_with_source(
                    ExecutionErrorKind::InsufficientCoinBalance,
                    format!("balance: {coin_value} required: {total}")
                )
            );
            values::coin_subtract_balance(coin_ref, total)?;
            let coins = amount_values
                .into_iter()
                .map(|a| context.new_coin(a))
                .collect::<Result<_, _>>()?;
            coins
        }
        T::Command::MergeCoins(_, target, coins) => {
            // TODO should we just call a Move function?
            let target_ref: Value = context.argument(target)?;
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
            let target_value = values::coin_value(context.copy_value(&target_ref)?)?;
            fp_ensure!(
                target_value.checked_add(additional).is_some(),
                ExecutionError::from_kind(ExecutionErrorKind::CoinBalanceOverflow,)
            );
            values::coin_add_balance(target_ref, additional)?;
            vec![]
        }
        T::Command::MakeMoveVec(ty, items) => {
            let items: Vec<Value> = context.arguments(items)?;
            vec![values::vec_pack(ty, items)?]
        }
        T::Command::Publish(..) => todo!("RUNTIME"),
        T::Command::Upgrade(..) => todo!("RUNTIME"),
    };
    context.result(result)?;
    Ok(())
}
