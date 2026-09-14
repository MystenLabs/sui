// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::natives::move_stdlib::vector::{
    KeepGasParameters, ReverseGasParameters, SliceGasParameters, SpliceGasParameters, keep_work,
    native_keep, native_reverse, native_slice, native_splice, splice_work,
};
use crate::{
    cache::identifier_interner::IdentifierInterner,
    execution::{
        dispatch_tables::VMDispatchTables,
        values::{INDEX_OUT_OF_BOUNDS, MemBox, VMValueCast, Value, VectorRef},
    },
    jit::execution::ast::Type,
    natives::{
        extensions::NativeContextExtensions,
        functions::{NativeContext, NativeResult},
    },
    shared::linkage_context::LinkageContext,
};
use move_core_types::{gas_algebra::InternalGas, vm_status::sub_status::NFE_OUT_OF_GAS};
use move_vm_config::runtime::VMConfig;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

fn with_native_context<R>(
    gas_budget: u64,
    f: impl FnOnce(&mut NativeContext<'_, '_, '_>) -> R,
) -> R {
    let vm_config = Arc::new(VMConfig::new_for_test(false, None));
    let runtime_limits = vm_config.runtime_limits_config.clone();
    let vtables = VMDispatchTables::new(
        vm_config,
        Arc::new(IdentifierInterner::new()),
        LinkageContext::new(BTreeMap::new()).unwrap(),
        BTreeMap::new(),
    )
    .unwrap();
    let mut extensions = NativeContextExtensions::default();
    let mut context = NativeContext::new(
        None,
        &vtables,
        &mut extensions,
        &runtime_limits,
        InternalGas::new(gas_budget),
    );
    f(&mut context)
}

fn assert_out_of_gas(result: crate::natives::functions::NativeResult, budget: u64) {
    assert!(matches!(result.result, Err(NFE_OUT_OF_GAS)));
    assert_eq!(result.cost, InternalGas::new(budget));
}

fn assert_success_cost(result: &NativeResult, cost: u64) {
    assert!(result.result.is_ok());
    assert_eq!(result.cost, InternalGas::new(cost));
}

fn assert_abort_cost(result: &NativeResult, cost: u64, abort_code: u64) {
    assert!(matches!(&result.result, Err(code) if *code == abort_code));
    assert_eq!(result.cost, InternalGas::new(cost));
}

#[test]
fn keep_work_counts_dropped_and_compacted_elements() {
    // Keeping a prefix drops only the suffix; no retained element is moved.
    assert_eq!(keep_work(5, 0, 3), (2, 0));
    // Keeping a middle range drops both sides and compacts the retained elements.
    assert_eq!(keep_work(5, 1, 4), (2, 3));
    // Keeping a suffix compacts the retained elements to the front.
    assert_eq!(keep_work(5, 2, 5), (2, 3));
    assert_eq!(keep_work(5, 0, 5), (0, 0));
    assert_eq!(keep_work(5, 2, 2), (5, 0));
}

#[test]
fn keep_work_is_zero_for_invalid_ranges() {
    assert_eq!(keep_work(5, 4, 3), (0, 0));
    assert_eq!(keep_work(5, 0, 6), (0, 0));
    assert_eq!(keep_work(5, u64::MAX, u64::MAX), (0, 0));
}

#[test]
fn splice_work_covers_valid_shapes() {
    // Equal-size replacement does not move the tail.
    assert_eq!(splice_work(8, 2, 5, 3), 3);
    // Growing and shrinking both move the tail when the sizes differ.
    assert_eq!(splice_work(8, 2, 4, 5), 9);
    assert_eq!(splice_work(8, 2, 6, 1), 3);
    // Appending has no tail to move. A suffix drain has no native relocation work; the VM gas
    // meter separately charges its returned vector by deep abstract size.
    assert_eq!(splice_work(8, 8, 8, 3), 3);
    assert_eq!(splice_work(8, 3, 8, 0), 0);
    // Draining from the middle moves only the tail. The returned value is charged separately.
    assert_eq!(splice_work(8, 2, 5, 0), 3);
}

#[test]
fn splice_work_is_zero_for_invalid_ranges() {
    assert_eq!(splice_work(8, 5, 4, 3), 0);
    assert_eq!(splice_work(8, 0, 9, 3), 0);
    assert_eq!(splice_work(8, u64::MAX, u64::MAX, 3), 0);
}

#[test]
fn successful_native_costs_match_work_formulas() {
    let reverse_holder = MemBox::new(Value::vector_u8([1, 2, 3, 4]));
    let reverse_inspect: VectorRef = VMValueCast::cast(reverse_holder.as_ref_value()).unwrap();
    let reverse_args = VecDeque::from([reverse_holder.as_ref_value()]);
    let reverse_params = ReverseGasParameters {
        base: 5.into(),
        per_elem: 3.into(),
    };
    let reverse_result = with_native_context(100, |context| {
        native_reverse(&reverse_params, context, vec![Type::U8], reverse_args).unwrap()
    });
    assert_success_cost(&reverse_result, 5 + 4 * 3);
    assert_eq!(*reverse_inspect.as_bytes_ref().unwrap(), vec![4, 3, 2, 1]);

    let keep_holder = MemBox::new(Value::vector_u8([1, 2, 3, 4, 5]));
    let keep_inspect: VectorRef = VMValueCast::cast(keep_holder.as_ref_value()).unwrap();
    let keep_args = VecDeque::from([keep_holder.as_ref_value(), Value::u64(1), Value::u64(4)]);
    let keep_params = KeepGasParameters {
        base: 5.into(),
        per_dropped_elem: 3.into(),
        per_moved_elem: 7.into(),
    };
    let keep_result = with_native_context(100, |context| {
        native_keep(&keep_params, context, vec![Type::U8], keep_args).unwrap()
    });
    assert_success_cost(&keep_result, 5 + 2 * 3 + 3 * 7);
    assert_eq!(*keep_inspect.as_bytes_ref().unwrap(), vec![2, 3, 4]);

    let splice_holder = MemBox::new(Value::vector_u8([1, 2, 3, 4]));
    let splice_inspect: VectorRef = VMValueCast::cast(splice_holder.as_ref_value()).unwrap();
    let splice_args = VecDeque::from([
        splice_holder.as_ref_value(),
        Value::u64(1),
        Value::u64(2),
        Value::vector_u8([9, 10]),
    ]);
    let splice_params = SpliceGasParameters {
        base: 5.into(),
        per_elem: 3.into(),
    };
    let splice_result = with_native_context(100, |context| {
        native_splice(&splice_params, context, vec![Type::U8], splice_args).unwrap()
    });
    assert_success_cost(&splice_result, 5 + 4 * 3);
    assert_eq!(
        *splice_inspect.as_bytes_ref().unwrap(),
        vec![1, 9, 10, 3, 4]
    );

    let slice_holder = MemBox::new(Value::vector_u8([1, 2, 3, 4]));
    let slice_args = VecDeque::from([slice_holder.as_ref_value(), Value::u64(1), Value::u64(3)]);
    let slice_result = with_native_context(100, |context| {
        native_slice(
            &SliceGasParameters { base: 5.into() },
            context,
            vec![Type::U8],
            slice_args,
        )
        .unwrap()
    });
    assert_success_cost(&slice_result, 5);
}

#[test]
fn invalid_ranges_charge_only_base() {
    let keep_holder = MemBox::new(Value::vector_u8([1, 2, 3, 4, 5]));
    let keep_inspect: VectorRef = VMValueCast::cast(keep_holder.as_ref_value()).unwrap();
    let keep_args = VecDeque::from([keep_holder.as_ref_value(), Value::u64(4), Value::u64(3)]);
    let keep_result = with_native_context(100, |context| {
        native_keep(
            &KeepGasParameters {
                base: 5.into(),
                per_dropped_elem: 3.into(),
                per_moved_elem: 7.into(),
            },
            context,
            vec![Type::U8],
            keep_args,
        )
        .unwrap()
    });
    assert_abort_cost(&keep_result, 5, INDEX_OUT_OF_BOUNDS);
    assert_eq!(*keep_inspect.as_bytes_ref().unwrap(), vec![1, 2, 3, 4, 5]);

    let splice_holder = MemBox::new(Value::vector_u8([1, 2, 3, 4]));
    let splice_inspect: VectorRef = VMValueCast::cast(splice_holder.as_ref_value()).unwrap();
    let splice_args = VecDeque::from([
        splice_holder.as_ref_value(),
        Value::u64(3),
        Value::u64(2),
        Value::vector_u8([9, 10]),
    ]);
    let splice_result = with_native_context(100, |context| {
        native_splice(
            &SpliceGasParameters {
                base: 5.into(),
                per_elem: 3.into(),
            },
            context,
            vec![Type::U8],
            splice_args,
        )
        .unwrap()
    });
    assert_abort_cost(&splice_result, 5, INDEX_OUT_OF_BOUNDS);
    assert_eq!(*splice_inspect.as_bytes_ref().unwrap(), vec![1, 2, 3, 4]);
}

#[test]
fn keep_full_range_charges_only_base() {
    let holder = MemBox::new(Value::vector_u8([1, 2, 3, 4, 5]));
    let inspect: VectorRef = VMValueCast::cast(holder.as_ref_value()).unwrap();
    let args = VecDeque::from([holder.as_ref_value(), Value::u64(0), Value::u64(5)]);
    let params = KeepGasParameters {
        base: 5.into(),
        per_dropped_elem: 3.into(),
        per_moved_elem: 7.into(),
    };

    let result = with_native_context(100, |context| {
        native_keep(&params, context, vec![Type::U8], args).unwrap()
    });

    assert_success_cost(&result, 5);
    assert_eq!(*inspect.as_bytes_ref().unwrap(), vec![1, 2, 3, 4, 5]);
}

#[test]
fn reverse_out_of_gas_does_not_mutate() {
    let holder = MemBox::new(Value::vector_u8([1, 2, 3, 4]));
    let inspect: VectorRef = VMValueCast::cast(holder.as_ref_value()).unwrap();
    let args = VecDeque::from([holder.as_ref_value()]);
    let params = ReverseGasParameters {
        base: 5.into(),
        per_elem: 3.into(),
    };

    let result = with_native_context(16, |context| {
        native_reverse(&params, context, vec![Type::U8], args).unwrap()
    });

    assert_out_of_gas(result, 16);
    assert_eq!(*inspect.as_bytes_ref().unwrap(), vec![1, 2, 3, 4]);
}

#[test]
fn keep_out_of_gas_does_not_mutate() {
    let holder = MemBox::new(Value::vector_u8([1, 2, 3, 4, 5]));
    let inspect: VectorRef = VMValueCast::cast(holder.as_ref_value()).unwrap();
    let args = VecDeque::from([holder.as_ref_value(), Value::u64(1), Value::u64(4)]);
    let params = KeepGasParameters {
        base: 5.into(),
        per_dropped_elem: 3.into(),
        per_moved_elem: 7.into(),
    };

    // Base and dropped-element charges consume this budget exactly; the moved-element charge
    // then fails before `VectorRef::keep` is called.
    let result = with_native_context(11, |context| {
        native_keep(&params, context, vec![Type::U8], args).unwrap()
    });

    assert_out_of_gas(result, 11);
    assert_eq!(*inspect.as_bytes_ref().unwrap(), vec![1, 2, 3, 4, 5]);
}

#[test]
fn splice_out_of_gas_does_not_mutate() {
    let holder = MemBox::new(Value::vector_u8([1, 2, 3, 4]));
    let inspect: VectorRef = VMValueCast::cast(holder.as_ref_value()).unwrap();
    let args = VecDeque::from([
        holder.as_ref_value(),
        Value::u64(1),
        Value::u64(2),
        Value::vector_u8([9, 10]),
    ]);
    let params = SpliceGasParameters {
        base: 5.into(),
        per_elem: 3.into(),
    };

    let result = with_native_context(16, |context| {
        native_splice(&params, context, vec![Type::U8], args).unwrap()
    });

    assert_out_of_gas(result, 16);
    assert_eq!(*inspect.as_bytes_ref().unwrap(), vec![1, 2, 3, 4]);
}
