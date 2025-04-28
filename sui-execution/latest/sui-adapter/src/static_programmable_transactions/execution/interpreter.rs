// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_value::ExecutionState,
    gas_charger::GasCharger,
    static_programmable_transactions::{env::Env, execution::context::Context, typing::ast as T},
};
use move_trace_format::format::MoveTraceBuilder;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Instant};
use sui_move_natives::object_runtime::ObjectRuntime;
use sui_types::{
    base_types::TxContext,
    error::ExecutionError,
    execution::{ExecutionTiming, ResultWithTimings},
    metrics::LimitsMetrics,
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
        if let Err(err) = execute_command(&mut context, command, tys, trace_builder_opt) {
            let object_runtime = context.object_runtime()?;
            // We still need to record the loaded child objects for replay
            let loaded_runtime_objects = object_runtime.loaded_runtime_objects();
            // we do not save the wrapped objects since on error, they should not be modified
            drop(context);
            // TODO wtf is going on with the borrow checker here
            let state_view: &mut dyn ExecutionState = unsafe { state_view.as_mut().unwrap() };
            state_view.save_loaded_runtime_objects(loaded_runtime_objects);
            timings.push(ExecutionTiming::Abort(start.elapsed()));
            return Err(err.with_command_index(idx));
        };
        timings.push(ExecutionTiming::Success(start.elapsed()));
    }
    // Save loaded objects table in case we fail in post execution
    let object_runtime: &ObjectRuntime = context.object_runtime()?;
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
    result_tys: T::ResultType,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> Result<(), ExecutionError> {
    match command {
        T::Command::MoveCall(move_call) => todo!(),
        T::Command::TransferObjects(items, _) => todo!(),
        T::Command::SplitCoins(_, _, items) => todo!(),
        T::Command::MergeCoins(_, _, items) => todo!(),
        T::Command::MakeMoveVec(_, items) => todo!(),
        T::Command::Publish(items, object_ids) => todo!("RUNTIME"),
        T::Command::Upgrade(items, object_ids, object_id, _) => todo!("RUNTIME"),
    }
}
