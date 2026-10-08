// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    NativesCostTable, get_extension, legacy_test_cost, object_runtime::ObjectRuntime,
    test_scenario::InMemoryTestStore,
};
use move_binary_format::errors::PartialVMResult;
use move_binary_format::safe_unwrap;
use move_core_types::gas_algebra::InternalGas;
use move_vm_runtime::{
    execution::{Type, values::Value},
    native_charge_gas_early_exit,
    natives::functions::{NativeContext, NativeResult},
    pop_arg,
};
use smallvec::smallvec;
use std::collections::VecDeque;

#[derive(Clone)]
pub struct ClockNowMsCostParams {
    pub clock_now_ms_cost_base: Option<InternalGas>,
}
/***************************************************************************************************
 * native fun native_now_ms
 * Implementation of the Move native function `fun native_now_ms(): u64`
 *   gas cost: clock_now_ms_cost_base | fixed cost; the Clock is a fixed-size object
 **************************************************************************************************/
pub fn now_ms(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    let clock_now_ms_cost_base = safe_unwrap!(
        get_extension!(context, NativesCostTable)?
            .clock_now_ms_cost_params
            .clock_now_ms_cost_base
    );
    native_charge_gas_early_exit!(context, clock_now_ms_cost_base);

    let object_runtime: &ObjectRuntime = get_extension!(context)?;
    let timestamp_ms = object_runtime.clock_timestamp_ms()?;

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::u64(timestamp_ms)],
    ))
}

/***************************************************************************************************
 * native fun native_set_now_ms_for_testing
 * Implementation of the Move native function
 * `fun native_set_now_ms_for_testing(timestamp_ms: u64)`
 **************************************************************************************************/
pub fn set_now_ms_for_testing(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let timestamp_ms = pop_arg!(args, u64);
    let store: &&InMemoryTestStore = get_extension!(context)?;
    store.set_clock_timestamp_ms(timestamp_ms);

    Ok(NativeResult::ok(legacy_test_cost(), smallvec![]))
}
