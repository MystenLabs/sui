// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    NativesCostTable, get_extension, get_extension_mut, legacy_test_cost,
    object_runtime::ObjectRuntime, test_scenario::InMemoryTestStore,
};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_binary_format::{safe_assert, safe_unwrap};
use move_core_types::{
    gas_algebra::InternalGas, language_storage::TypeTag, runtime_value as R, vm_status::StatusCode,
};
use move_vm_runtime::{
    execution::{Type, values::Value},
    native_charge_gas_early_exit,
    natives::functions::{NativeContext, NativeResult},
    pop_arg,
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::{SUI_CLOCK_OBJECT_ID, clock::Clock, id::UID};

const E_CLOCK_ALREADY_EXISTS: u64 = 1;

#[derive(Clone)]
pub struct ClockBorrowCostParams {
    pub clock_borrow_cost_base: Option<InternalGas>,
}
/***************************************************************************************************
 * native fun native_borrow
 * Implementation of the Move native function `fun native_borrow(): &Clock`
 *   gas cost: clock_borrow_cost_base | fixed cost; the Clock is a fixed-size object
 **************************************************************************************************/
pub fn borrow(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert!(ty_args.is_empty());
    safe_assert!(args.is_empty());

    let clock_borrow_cost_base = safe_unwrap!(
        get_extension!(context, NativesCostTable)?
            .clock_borrow_cost_params
            .clock_borrow_cost_base
    );
    native_charge_gas_early_exit!(context, clock_borrow_cost_base);

    let layout = clock_layout(context)?;
    let object_runtime: &mut ObjectRuntime = get_extension_mut!(context)?;
    let clock_ref = object_runtime.borrow_clock(&layout)?;

    Ok(NativeResult::ok(context.gas_used(), smallvec![clock_ref]))
}

fn clock_layout(context: &NativeContext) -> PartialVMResult<R::MoveTypeLayout> {
    context
        .type_tag_to_type_layout(&TypeTag::Struct(Box::new(Clock::type_())))
        .ok_or_else(|| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("Failed to load the Clock type layout".to_string())
        })
}

/***************************************************************************************************
 * native fun native_set_timestamp_ms_for_testing
 * Implementation of the Move native function
 * `fun native_set_timestamp_ms_for_testing(timestamp_ms: u64)`
 **************************************************************************************************/
pub fn set_timestamp_ms_for_testing(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert!(ty_args.is_empty());
    safe_assert!(args.len() == 1);

    let timestamp_ms = pop_arg!(args, u64);
    let store: &&InMemoryTestStore = get_extension!(context)?;
    store.set_clock_timestamp_ms(timestamp_ms);
    let object_runtime: &mut ObjectRuntime = get_extension_mut!(context)?;
    object_runtime.clear_clock_for_testing();

    Ok(NativeResult::ok(legacy_test_cost(), smallvec![]))
}

/***************************************************************************************************
 * native fun native_create_for_testing
 * Implementation of the Move native function `fun native_create_for_testing(): Clock`
 **************************************************************************************************/
pub fn create_for_testing(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert!(ty_args.is_empty());
    safe_assert!(args.is_empty());

    let store: &&InMemoryTestStore = get_extension!(context)?;
    if store.clock_exists() {
        return Ok(NativeResult::err(
            legacy_test_cost(),
            E_CLOCK_ALREADY_EXISTS,
        ));
    }
    store.set_clock_timestamp_ms(0);
    let layout = clock_layout(context)?;
    let object_runtime: &mut ObjectRuntime = get_extension_mut!(context)?;
    // The Clock's id is fixed, so register it the way `object::new` does for fresh ids: deleting
    // and recreating it within one transaction then leaves it neither created nor deleted.
    object_runtime.new_id(SUI_CLOCK_OBJECT_ID)?;
    object_runtime.clear_clock_for_testing();

    let clock = Clock {
        id: UID::new(SUI_CLOCK_OBJECT_ID),
        timestamp_ms: 0,
    };
    let bytes = bcs::to_bytes(&clock).map_err(|e| {
        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
            .with_message(format!("Failed to serialize the Clock: {e}"))
    })?;
    let value = Value::simple_deserialize(&bytes, &layout).ok_or_else(|| {
        PartialVMError::new(StatusCode::FAILED_TO_DESERIALIZE_RESOURCE)
            .with_message("Failed to deserialize the Clock".to_string())
    })?;

    Ok(NativeResult::ok(legacy_test_cost(), smallvec![value]))
}

/***************************************************************************************************
 * native fun native_clear_for_testing
 * Implementation of the Move native function `fun native_clear_for_testing()`
 **************************************************************************************************/
pub fn clear_for_testing(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    safe_assert!(ty_args.is_empty());
    safe_assert!(args.is_empty());

    let store: &&InMemoryTestStore = get_extension!(context)?;
    store.clear_clock_timestamp_ms();
    let object_runtime: &mut ObjectRuntime = get_extension_mut!(context)?;
    object_runtime.clear_clock_for_testing();

    Ok(NativeResult::ok(legacy_test_cost(), smallvec![]))
}
