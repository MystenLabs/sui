// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution::values::{Value, Vector, VectorRef},
    jit::execution::ast::Type,
    native_charge_gas_early_exit,
    natives::{
        functions::{NativeContext, NativeFunction, NativeResult},
        make_module_natives,
    },
    pop_arg,
    shared::{
        safe_ops::SafeIndex as _,
        views::{SizeConfig, ValueView},
    },
};
use move_binary_format::{
    checked_as,
    errors::{PartialVMError, PartialVMResult},
    partial_vm_error,
};
use move_core_types::{
    gas_algebra::{InternalGas, InternalGasPerAbstractMemoryUnit, InternalGasPerArg, NumArgs},
    vm_status::StatusCode,
};
use std::{collections::VecDeque, sync::Arc};

/***************************************************************************************************
 * native fun empty
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct EmptyGasParameters {
    pub base: InternalGas,
}

pub fn native_empty(
    gas_params: &EmptyGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.is_empty());

    native_charge_gas_early_exit!(context, gas_params.base);

    let ty = ty_args.first().ok_or_else(|| {
        partial_vm_error!(
            UNKNOWN_INVARIANT_VIOLATION_ERROR,
            "native vector::empty must have one type argument"
        )
    })?;

    NativeResult::map_partial_vm_result_one(
        context.gas_used(),
        ty.try_into().and_then(Vector::empty),
    )
}

pub fn make_native_empty(gas_params: EmptyGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_empty(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun length
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct LengthGasParameters {
    pub base: InternalGas,
}

pub fn native_length(
    gas_params: &LengthGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    native_charge_gas_early_exit!(context, gas_params.base);

    let r = pop_arg!(args, VectorRef);
    NativeResult::map_partial_vm_result_one(context.gas_used(), r.len(ty_args.safe_get(0)?))
}

pub fn make_native_length(gas_params: LengthGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_length(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun push_back
 *
 *   gas cost: base_cost + legacy_unit_cost * max(1, size_of(val))
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct PushBackGasParameters {
    pub base: InternalGas,
    pub legacy_per_abstract_memory_unit: InternalGasPerAbstractMemoryUnit,
}

pub fn native_push_back(
    gas_params: &PushBackGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 2);

    native_charge_gas_early_exit!(context, gas_params.base);

    let Some(e) = args.pop_back() else {
        return Err(partial_vm_error!(
            VECTOR_OPERATION_ERROR,
            "Internal error: missing argument in native push_back"
        ));
    };

    let r = pop_arg!(args, VectorRef);

    if gas_params.legacy_per_abstract_memory_unit != 0.into() {
        let size = e.abstract_memory_size(&SizeConfig {
            traverse_references: false,
            include_vector_size: false,
        })?;
        let cost = gas_params.legacy_per_abstract_memory_unit * std::cmp::max(size, 1.into());
        native_charge_gas_early_exit!(context, cost);
    }

    NativeResult::map_partial_vm_result_empty(
        context.gas_used(),
        r.push_back(
            e,
            ty_args.safe_get(0)?,
            context.runtime_limits_config().vector_len_max,
        ),
    )
}

pub fn make_native_push_back(gas_params: PushBackGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_push_back(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun borrow
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct BorrowGasParameters {
    pub base: InternalGas,
}

pub fn native_borrow(
    gas_params: &BorrowGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 2);

    native_charge_gas_early_exit!(context, gas_params.base);
    let idx = checked_as!(pop_arg!(args, u64), usize)?;
    let r = pop_arg!(args, VectorRef);
    NativeResult::map_partial_vm_result_one(
        context.gas_used(),
        r.borrow_elem(idx, ty_args.safe_get(0)?)
            .map_err(native_error_to_abort),
    )
}

pub fn make_native_borrow(gas_params: BorrowGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_borrow(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun pop
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct PopBackGasParameters {
    pub base: InternalGas,
}

pub fn native_pop_back(
    gas_params: &PopBackGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    native_charge_gas_early_exit!(context, gas_params.base);
    let r = pop_arg!(args, VectorRef);
    NativeResult::map_partial_vm_result_one(
        context.gas_used(),
        r.pop(ty_args.safe_get(0)?).map_err(native_error_to_abort),
    )
}

pub fn make_native_pop_back(gas_params: PopBackGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_pop_back(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun destroy_empty
 *
 *   gas cost: base_cost
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct DestroyEmptyGasParameters {
    pub base: InternalGas,
}

pub fn native_destroy_empty(
    gas_params: &DestroyEmptyGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    native_charge_gas_early_exit!(context, gas_params.base);

    let v = pop_arg!(args, Vector);
    NativeResult::map_partial_vm_result_empty(
        context.gas_used(),
        v.destroy_empty(ty_args.safe_get(0)?)
            .map_err(native_error_to_abort),
    )
}

pub fn make_native_destroy_empty(gas_params: DestroyEmptyGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_destroy_empty(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun swap
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct SwapGasParameters {
    pub base: InternalGas,
}

pub fn native_swap(
    gas_params: &SwapGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 3);

    native_charge_gas_early_exit!(context, gas_params.base);
    let idx2 = checked_as!(pop_arg!(args, u64), usize)?;
    let idx1 = checked_as!(pop_arg!(args, u64), usize)?;
    let r = pop_arg!(args, VectorRef);
    NativeResult::map_partial_vm_result_empty(
        context.gas_used(),
        r.swap(idx1, idx2, ty_args.safe_get(0)?)
            .map_err(native_error_to_abort),
    )
}

pub fn make_native_swap(gas_params: SwapGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_swap(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun reverse
 *
 *   gas cost: base_cost + per_elem * num_elements
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct ReverseGasParameters {
    pub base: InternalGas,
    pub per_elem: InternalGasPerArg,
}

pub fn native_reverse(
    gas_params: &ReverseGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 1);

    native_charge_gas_early_exit!(context, gas_params.base);

    let v = pop_arg!(args, VectorRef);
    let ty = ty_args.safe_get(0)?;
    let len = match vector_len(&v, ty) {
        Ok(len) => len,
        Err(error) => {
            return NativeResult::map_partial_vm_result_empty(
                context.gas_used(),
                Err(native_error_to_abort(error)),
            );
        }
    };
    native_charge_gas_early_exit!(context, gas_params.per_elem * NumArgs::new(len));

    NativeResult::map_partial_vm_result_empty(
        context.gas_used(),
        v.reverse(ty).map_err(native_error_to_abort),
    )
}

pub fn make_native_reverse(gas_params: ReverseGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_reverse(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun keep
 *
 *   gas cost: base_cost + per_dropped_elem * dropped
 *       + per_moved_elem * retained elements moved
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct KeepGasParameters {
    pub base: InternalGas,
    pub per_dropped_elem: InternalGasPerArg,
    pub per_moved_elem: InternalGasPerArg,
}

pub(crate) fn keep_work(len: u64, start: u64, end: u64) -> (u64, u64) {
    if start > end || end > len {
        return (0, 0);
    }

    let retained = end - start;
    let dropped = len - retained;
    let moved = if start == 0 { 0 } else { retained };
    (dropped, moved)
}

pub fn native_keep(
    gas_params: &KeepGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 3);

    native_charge_gas_early_exit!(context, gas_params.base);

    let end_arg = pop_arg!(args, u64);
    let start_arg = pop_arg!(args, u64);
    let end = checked_as!(end_arg, usize)?;
    let start = checked_as!(start_arg, usize)?;
    let v = pop_arg!(args, VectorRef);
    let ty = ty_args.safe_get(0)?;

    let len = match vector_len(&v, ty) {
        Ok(len) => len,
        Err(error) => {
            return NativeResult::map_partial_vm_result_empty(
                context.gas_used(),
                Err(native_error_to_abort(error)),
            );
        }
    };
    // `VectorRef::keep` performs the actual validation. Do not charge per-element work for a
    // range which it will reject.
    let (dropped, moved) = keep_work(len, start_arg, end_arg);
    native_charge_gas_early_exit!(context, gas_params.per_dropped_elem * NumArgs::new(dropped));
    native_charge_gas_early_exit!(context, gas_params.per_moved_elem * NumArgs::new(moved));

    NativeResult::map_partial_vm_result_empty(
        context.gas_used(),
        v.keep(start, end, ty).map_err(native_error_to_abort),
    )
}

pub fn make_native_keep(gas_params: KeepGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_keep(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun slice
 *
 *   native gas cost: base_cost
 *   The VM gas meter separately charges the returned vector by its deep abstract size.
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct SliceGasParameters {
    pub base: InternalGas,
}

pub fn native_slice(
    gas_params: &SliceGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 3);

    native_charge_gas_early_exit!(context, gas_params.base);

    let j = checked_as!(pop_arg!(args, u64), usize)?;
    let i = checked_as!(pop_arg!(args, u64), usize)?;
    let v = pop_arg!(args, VectorRef);

    NativeResult::map_partial_vm_result_one(
        context.gas_used(),
        v.slice(i, j, ty_args.safe_get(0)?)
            .map(Vector::into_value)
            .map_err(native_error_to_abort),
    )
}

pub fn make_native_slice(gas_params: SliceGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_slice(&gas_params, context, ty_args, args)
        },
    )
}

/***************************************************************************************************
 * native fun splice
 *
 *   native gas cost: base_cost + per_elem * (num_inserted + tail_moved)
 *   The VM gas meter separately charges the returned, removed vector by its deep abstract size.
 *
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct SpliceGasParameters {
    pub base: InternalGas,
    pub per_elem: InternalGasPerArg,
}

pub(crate) fn splice_work(len: u64, i: u64, j: u64, n_in: u64) -> u64 {
    if i > j || j > len {
        return 0;
    }

    let n_removed = j - i;
    let tail_moved = if n_in == n_removed { 0 } else { len - j };
    n_in.saturating_add(tail_moved)
}

pub fn native_splice(
    gas_params: &SpliceGasParameters,
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.len() == 1);
    debug_assert!(args.len() == 4);

    native_charge_gas_early_exit!(context, gas_params.base);

    // get arguments from the Move call frame
    let other = pop_arg!(args, Vector);
    let j_arg = pop_arg!(args, u64);
    let i_arg = pop_arg!(args, u64);
    let j = checked_as!(j_arg, usize)?;
    let i = checked_as!(i_arg, usize)?;
    let v = pop_arg!(args, VectorRef);

    // charge according to the moved elements (relocation)
    let n_in = other.elem_len()? as u64;
    let ty = ty_args.safe_get(0)?;
    let len = match vector_len(&v, ty) {
        Ok(len) => len,
        Err(error) => {
            return NativeResult::map_partial_vm_result_one(
                context.gas_used(),
                Err(native_error_to_abort(error)),
            );
        }
    };
    native_charge_gas_early_exit!(
        context,
        gas_params.per_elem * NumArgs::new(splice_work(len, i_arg, j_arg, n_in))
    );

    NativeResult::map_partial_vm_result_one(
        context.gas_used(),
        v.splice(
            i,
            j,
            other,
            ty,
            context.runtime_limits_config().vector_len_max,
        )
        .map(Vector::into_value)
        .map_err(native_error_to_abort),
    )
}

pub fn make_native_splice(gas_params: SpliceGasParameters) -> NativeFunction {
    Arc::new(
        move |context, ty_args, args| -> PartialVMResult<NativeResult> {
            native_splice(&gas_params, context, ty_args, args)
        },
    )
}

fn vector_len(v: &VectorRef, ty: &Type) -> PartialVMResult<u64> {
    match v.len(ty)? {
        Value::U64(len) => Ok(len),
        _ => Err(partial_vm_error!(
            UNKNOWN_INVARIANT_VIOLATION_ERROR,
            "vector::length must return a u64"
        )),
    }
}

fn native_error_to_abort(err: PartialVMError) -> PartialVMError {
    let (major_status, sub_status_opt, message_opt, exec_state_opt, indices, offsets) =
        err.all_data();
    let new_err = match major_status {
        StatusCode::VECTOR_OPERATION_ERROR => partial_vm_error!(ABORTED),
        _ => PartialVMError::new(major_status),
    };
    let new_err = match sub_status_opt {
        None => new_err,
        Some(code) => new_err.with_sub_status(code),
    };
    let new_err = match message_opt {
        None => new_err,
        Some(message) => new_err.with_message(message),
    };
    let new_err = match exec_state_opt {
        None => new_err,
        Some(stacktrace) => new_err.with_exec_state(stacktrace),
    };
    new_err.at_indices(indices).at_code_offsets(offsets)
}

/***************************************************************************************************
 * module
 **************************************************************************************************/
#[derive(Debug, Clone)]
pub struct GasParameters {
    pub empty: EmptyGasParameters,
    pub length: LengthGasParameters,
    pub push_back: PushBackGasParameters,
    pub borrow: BorrowGasParameters,
    pub pop_back: PopBackGasParameters,
    pub destroy_empty: DestroyEmptyGasParameters,
    pub swap: SwapGasParameters,
    pub reverse: ReverseGasParameters,
    pub keep: KeepGasParameters,
    pub slice: SliceGasParameters,
    pub splice: SpliceGasParameters,
}

pub fn make_all(gas_params: GasParameters) -> impl Iterator<Item = (String, NativeFunction)> {
    let natives = [
        ("empty", make_native_empty(gas_params.empty)),
        ("length", make_native_length(gas_params.length)),
        ("push_back", make_native_push_back(gas_params.push_back)),
        ("borrow", make_native_borrow(gas_params.borrow.clone())),
        ("borrow_mut", make_native_borrow(gas_params.borrow)),
        ("pop_back", make_native_pop_back(gas_params.pop_back)),
        (
            "destroy_empty",
            make_native_destroy_empty(gas_params.destroy_empty),
        ),
        ("swap", make_native_swap(gas_params.swap)),
        ("reverse", make_native_reverse(gas_params.reverse)),
        ("keep", make_native_keep(gas_params.keep)),
        ("slice", make_native_slice(gas_params.slice)),
        ("splice", make_native_splice(gas_params.splice)),
    ];

    make_module_natives(natives)
}
