// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution::{
        dispatch_tables::VMDispatchTables,
        interpreter::state::{CallStack, MachineState},
        tracing::trace,
        values::Value,
    },
    jit::execution::ast::{Function, Type},
    natives::extensions::NativeContextExtensions,
    record_time,
    runtime::telemetry::TransactionTelemetryContext,
    shared::{gas::GasMeter, vm_pointer::VMPointer},
};
use move_binary_format::errors::*;
use move_vm_config::runtime::VMConfig;
use move_vm_profiler::{profile_close_frame, profile_open_frame};
use std::sync::Arc;

mod eval;
pub(crate) mod helpers;
pub mod locals;
pub(crate) mod state;

/// Entrypoint into the interpreter. All external calls need to be routed through this
/// function.
pub(crate) fn run(
    telemetry: &mut TransactionTelemetryContext,
    vm_config: Arc<VMConfig>,
    extensions: &mut NativeContextExtensions,
    tracer: &mut Option<VMTracer<'_>>,
    gas_meter: &mut impl GasMeter,
    vtables: &mut VMDispatchTables,
    function: VMPointer<Function>,
    ty_args: Vec<Type>,
    args: Vec<Value>,
) -> VMResult<Vec<Value>> {
    record_time!(INTERPRETER ; telemetry => {
        let fun_ref = function.to_ref();
        trace(tracer, |tracer| {
            tracer.enter_initial_frame(
                vtables,
                &gas_meter.remaining_gas().into(),
                function.ptr_clone().to_ref(),
                &ty_args,
                &args,
            )
        });
        profile_open_frame!(gas_meter, fun_ref.pretty_string());

        if fun_ref.is_native() {
            let return_result = eval::call_native_with_args(
                None,
                vtables,
                gas_meter,
                &vm_config.runtime_limits_config,
                extensions,
                fun_ref,
                &ty_args,
                args.into(),
            )
            .map_err(|e| {
                e.at_code_offset(fun_ref.index(), 0)
                    .finish(Location::Module(fun_ref.module_id().clone()))
            });
            trace(tracer, |tracer| {
                tracer.exit_initial_native_frame(&return_result, &gas_meter.remaining_gas().into())
            });
            profile_close_frame!(gas_meter, fun_ref.pretty_string());
            return_result.map(|values| values.into_iter().collect())
        } else {
            let call_stack = CallStack::new(function, ty_args, args).map_err(|e| {
                e.at_code_offset(fun_ref.index(), 0)
                    .finish(Location::Module(fun_ref.module_id().clone()))
            })?;
            let state = MachineState::new(call_stack);
            eval::run(state, vtables, vm_config, extensions, tracer, gas_meter)
        }
    })
}

macro_rules! set_err_info {
    ($frame:expr, $e:expr) => {{
        let function = $frame.function();
        $e.at_code_offset(function.index(), $frame.pc)
            .finish($frame.location())
    }};
}

pub(crate) use set_err_info;

use super::tracing::tracer::VMTracer;
