// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use std::collections::BTreeMap;

use move_binary_format::errors::{PartialVMError, PartialVMResult};

use move_core_types::gas_algebra::{AbstractMemorySize, InternalGas, NumArgs, NumBytes};
use move_core_types::language_storage::{ModuleId, TypeTag};

use move_core_types::vm_status::StatusCode;
use move_vm_types::gas::{GasMeter, SimpleInstruction};
use move_vm_types::loaded_data::runtime_types::Type;
use move_vm_types::views::{TypeView, ValueView, ValueVisitor};
use once_cell::sync::Lazy;

use crate::tier_based::units_types::{CostTable, Gas, GasCost};

use crate::tier_based::tables as T;
use crate::tier_based::units_types as TU;

pub use crate::tier_based::tables::initial_cost_schedule_v1;
pub use crate::tier_based::tables::initial_cost_schedule_v2;
pub use crate::tier_based::tables::initial_cost_schedule_v3;

pub static ZERO_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(T::zero_cost_schedule);

#[derive(Debug)]
pub struct GasStatus {
    pub current: T::GasStatus,
    pub new: T::GasStatus,
    start_time: Instant,
}

impl GasStatus {
    /// Initialize the gas state with metering enabled.
    ///
    /// Charge for every operation and fail when there is no more gas to pay for operations.
    /// This is the instantiation that must be used when executing a user script.

    pub fn new_v2(
        cost_table: CostTable,
        budget: u64,
        gas_price: u64,
        gas_model_version: u64,
    ) -> Self {
        Self {
            current: T::GasStatus::new_v2(cost_table, budget, gas_price, gas_model_version),
            new: T::GasStatus::new_v2(initial_cost_schedule_v3(), u32::MAX as u64, gas_price, 5),
            start_time: Instant::now(),
        }
    }

    pub fn new(cost_table: CostTable, gas_left: Gas) -> Self {
        Self {
            current: T::GasStatus::new(cost_table, gas_left),
            new: T::GasStatus::new(initial_cost_schedule_v2(), gas_left),
            start_time: Instant::now(),
        }
    }

    /// Initialize the gas state with metering disabled.
    ///
    /// It should be used by clients in very specific cases and when executing system
    /// code that does not have to charge the user.
    pub fn new_unmetered() -> Self {
        Self {
            current: T::GasStatus::new_unmetered(),
            new: T::GasStatus::new_unmetered(),
            start_time: Instant::now(),
        }
    }

    const INTERNAL_UNIT_MULTIPLIER: u64 = 1000;

    fn to_internal_units(val: u64) -> InternalGas {
        InternalGas::new(val * Self::INTERNAL_UNIT_MULTIPLIER)
    }

    #[allow(dead_code)]
    pub(crate) fn to_mist(&self, val: InternalGas) -> u64 {
        self.current.to_mist(val)
    }

    pub fn push_stack(&mut self, pushes: u64) -> PartialVMResult<()> {
        self.new.push_stack(pushes).unwrap();
        self.current.push_stack(pushes)
    }

    pub fn pop_stack(&mut self, pops: u64) {
        self.new.pop_stack(pops);
        self.current.pop_stack(pops)
    }

    pub fn increase_instruction_count(&mut self, amount: u64) -> PartialVMResult<()> {
        self.new.increase_instruction_count(amount).unwrap();
        self.current.increase_instruction_count(amount)
    }

    pub fn increase_stack_size(&mut self, size_amount: u64) -> PartialVMResult<()> {
        self.new.increase_stack_size(size_amount).unwrap();
        self.current.increase_stack_size(size_amount)
    }

    pub fn decrease_stack_size(&mut self, size_amount: u64) {
        self.new.decrease_stack_size(size_amount);
        self.current.decrease_stack_size(size_amount)
    }

    /// Given: pushes + pops + increase + decrease in size for an instruction charge for the
    /// execution of the instruction.
    pub fn charge(
        &mut self,
        num_instructions: u64,
        pushes: u64,
        pops: u64,
        incr_size: u64,
        decr_size: u64,
    ) -> PartialVMResult<()> {
        self.new
            .charge(num_instructions, pushes, pops, incr_size, decr_size)
            .unwrap();
        self.current
            .charge(num_instructions, pushes, pops, incr_size, decr_size)
    }

    /// Return the `CostTable` behind this `GasStatus`.
    pub fn cost_table(&self) -> &CostTable {
        &self.current.cost_table
    }

    /// Return the gas left.
    pub fn remaining_gas(&self) -> Gas {
        self.current.gas_left.to_unit_round_down()
    }

    /// Charge a given amount of gas and fail if not enough gas units are left.
    pub fn deduct_gas(&mut self, amount: InternalGas) -> PartialVMResult<()> {
        self.new.deduct_gas(amount).unwrap();
        self.current.deduct_gas(amount)
    }

    // Deduct the amount provided with no conversion, as if it was InternalGasUnit
    fn deduct_units(&mut self, amount: u64) -> PartialVMResult<()> {
        self.new.deduct_units(amount).unwrap();
        self.current.deduct_units(amount)
    }

    pub fn set_metering(&mut self, enabled: bool) {
        self.new.set_metering(enabled);
        self.current.set_metering(enabled)
    }

    // The amount of gas used, it does not include the multiplication for the gas price
    pub fn gas_used_pre_gas_price(&self) -> u64 {
        self.current.gas_used_pre_gas_price()
    }

    // Charge the number of bytes with the cost per byte value
    // As more bytes are read throughout the computation the cost per bytes is increased.
    pub fn charge_bytes(&mut self, size: usize, cost_per_byte: u64) -> PartialVMResult<()> {
        self.new.charge_bytes(size, cost_per_byte).unwrap();
        self.current.charge_bytes(size, cost_per_byte)
    }

    pub fn time(&self) -> u128 {
        self.start_time.elapsed().as_micros()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MyVal(pub u64);

#[derive(Debug, Clone)]
pub struct MyTag(pub TypeTag);

// Ooohhh so hacky but also somewhat wonderful
impl ValueView for MyVal {
    fn visit(&self, visitor: &mut impl ValueVisitor) {
        std::unimplemented!()
    }

    fn legacy_abstract_memory_size(&self) -> AbstractMemorySize {
        AbstractMemorySize::new(self.0)
    }
}

impl TypeView for MyTag {
    fn to_type_tag(&self) -> TypeTag {
        self.0.clone()
    }
}

impl MyVal {
    pub fn val(x: impl ValueView) -> Self {
        Self(u64::from(x.legacy_abstract_memory_size()))
    }

    pub fn ref_(x: &impl ValueView) -> Self {
        Self(u64::from(x.legacy_abstract_memory_size()))
    }
}

impl MyTag {
    pub fn val(x: impl TypeView) -> Self {
        Self(x.to_type_tag())
    }

    pub fn ref_(x: &impl TypeView) -> Self {
        Self(x.to_type_tag())
    }
}

impl GasMeter for GasStatus {
    fn charge_simple_instr(
        &mut self,
        instr: move_vm_types::gas::SimpleInstruction,
    ) -> PartialVMResult<()> {
        self.new.charge_simple_instr(instr);
        self.current.charge_simple_instr(instr)
    }

    fn charge_pop(
        &mut self,
        popped_val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        self.new.charge_pop(MyVal::ref_(&popped_val));
        self.current.charge_pop(MyVal::ref_(&popped_val))
    }

    fn charge_call(
        &mut self,
        module_id: &move_core_types::language_storage::ModuleId,
        func_name: &str,
        args: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
        num_locals: move_core_types::gas_algebra::NumArgs,
    ) -> PartialVMResult<()> {
        let args: Vec<_> = args.map(MyVal::val).collect();
        self.new
            .charge_call(module_id, func_name, args.clone().into_iter(), num_locals);
        self.current
            .charge_call(module_id, func_name, args.into_iter(), num_locals)
    }

    fn charge_call_generic(
        &mut self,
        module_id: &move_core_types::language_storage::ModuleId,
        func_name: &str,
        ty_args: impl ExactSizeIterator<Item = impl move_vm_types::views::TypeView>,
        args: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
        num_locals: move_core_types::gas_algebra::NumArgs,
    ) -> PartialVMResult<()> {
        let ty_args: Vec<_> = ty_args.map(MyTag::val).collect();
        let args: Vec<_> = args.map(MyVal::val).collect();
        self.new.charge_call_generic(
            module_id,
            func_name,
            ty_args.clone().into_iter(),
            args.clone().into_iter(),
            num_locals,
        );
        self.current.charge_call_generic(
            module_id,
            func_name,
            ty_args.into_iter(),
            args.into_iter(),
            num_locals,
        )
    }

    fn charge_ld_const(
        &mut self,
        size: move_core_types::gas_algebra::NumBytes,
    ) -> PartialVMResult<()> {
        self.new.charge_ld_const(size);
        self.current.charge_ld_const(size)
    }

    fn charge_ld_const_after_deserialization(
        &mut self,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let val = MyVal::val(val);
        self.new.charge_ld_const_after_deserialization(val);
        self.current.charge_ld_const_after_deserialization(val)
    }

    fn charge_copy_loc(
        &mut self,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let val = MyVal::val(val);
        self.new.charge_copy_loc(val);
        self.current.charge_copy_loc(val)
    }

    fn charge_move_loc(
        &mut self,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let val = MyVal::val(val);
        self.new.charge_move_loc(val);
        self.current.charge_move_loc(val)
    }

    fn charge_store_loc(
        &mut self,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let val = MyVal::val(val);
        self.new.charge_store_loc(val);
        self.current.charge_store_loc(val)
    }

    fn charge_pack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let args: Vec<_> = args.map(MyVal::val).collect();
        self.new.charge_pack(is_generic, args.clone().into_iter());
        self.current.charge_pack(is_generic, args.into_iter())
    }

    fn charge_unpack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let args: Vec<_> = args.map(MyVal::val).collect();
        self.new.charge_unpack(is_generic, args.clone().into_iter());
        self.current.charge_unpack(is_generic, args.into_iter())
    }

    fn charge_read_ref(
        &mut self,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let val = MyVal::val(val);
        self.new.charge_read_ref(val);
        self.current.charge_read_ref(val)
    }

    fn charge_write_ref(
        &mut self,
        new_val: impl move_vm_types::views::ValueView,
        old_val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let new_val = MyVal::val(new_val);
        let old_val = MyVal::val(old_val);
        self.new.charge_write_ref(new_val, old_val);
        self.current.charge_write_ref(new_val, old_val)
    }

    fn charge_eq(
        &mut self,
        lhs: impl move_vm_types::views::ValueView,
        rhs: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let lhs = MyVal::val(lhs);
        let rhs = MyVal::val(rhs);
        self.new.charge_eq(lhs, rhs);
        self.current.charge_eq(lhs, rhs)
    }

    fn charge_neq(
        &mut self,
        lhs: impl move_vm_types::views::ValueView,
        rhs: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let lhs = MyVal::val(lhs);
        let rhs = MyVal::val(rhs);
        self.new.charge_neq(lhs, rhs);
        self.current.charge_neq(lhs, rhs)
    }

    fn charge_borrow_global(
        &mut self,
        is_mut: bool,
        is_generic: bool,
        ty: impl move_vm_types::views::TypeView,
        is_success: bool,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        self.new
            .charge_borrow_global(is_mut, is_generic, ty.clone(), is_success);
        self.current
            .charge_borrow_global(is_mut, is_generic, ty, is_success)
    }

    fn charge_exists(
        &mut self,
        is_generic: bool,
        ty: impl move_vm_types::views::TypeView,
        // TODO(Gas): see if we can get rid of this param
        exists: bool,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        self.current.charge_exists(is_generic, ty.clone(), exists);
        self.new.charge_exists(is_generic, ty, exists)
    }

    fn charge_move_from(
        &mut self,
        is_generic: bool,
        ty: impl move_vm_types::views::TypeView,
        val: Option<impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        let val = val.map(MyVal::val);
        self.current
            .charge_move_from(is_generic, ty.clone(), val.clone());
        self.new.charge_move_from(is_generic, ty, val)
    }

    fn charge_move_to(
        &mut self,
        is_generic: bool,
        ty: impl move_vm_types::views::TypeView,
        val: impl move_vm_types::views::ValueView,
        is_success: bool,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        let val = MyVal::val(val);
        self.new
            .charge_move_to(is_generic, ty.clone(), val.clone(), is_success);
        self.current.charge_move_to(is_generic, ty, val, is_success)
    }

    fn charge_vec_pack<'b>(
        &mut self,
        ty: impl move_vm_types::views::TypeView + 'b,
        args: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        let args: Vec<_> = args.map(MyVal::val).collect();
        self.new
            .charge_vec_pack(ty.clone(), args.clone().into_iter());
        self.current.charge_vec_pack(ty, args.into_iter())
    }

    fn charge_vec_len(&mut self, ty: impl move_vm_types::views::TypeView) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        self.new.charge_vec_len(ty.clone());
        self.current.charge_vec_len(ty)
    }

    fn charge_vec_borrow(
        &mut self,
        is_mut: bool,
        ty: impl move_vm_types::views::TypeView,
        is_success: bool,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        self.new.charge_vec_borrow(is_mut, ty.clone(), is_success);
        self.current.charge_vec_borrow(is_mut, ty, is_success)
    }

    fn charge_vec_push_back(
        &mut self,
        ty: impl move_vm_types::views::TypeView,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        let val = MyVal::val(val);
        self.new.charge_vec_push_back(ty.clone(), val.clone());
        self.current.charge_vec_push_back(ty, val)
    }

    fn charge_vec_pop_back(
        &mut self,
        ty: impl move_vm_types::views::TypeView,
        val: Option<impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        let val = val.map(MyVal::val);
        self.new.charge_vec_pop_back(ty.clone(), val.clone());
        self.current.charge_vec_pop_back(ty, val)
    }

    fn charge_vec_unpack(
        &mut self,
        ty: impl move_vm_types::views::TypeView,
        expect_num_elements: move_core_types::gas_algebra::NumArgs,
        elems: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        let elems: Vec<_> = elems.map(MyVal::val).collect();
        self.new
            .charge_vec_unpack(ty.clone(), expect_num_elements, elems.clone().into_iter());
        self.current
            .charge_vec_unpack(ty, expect_num_elements, elems.into_iter())
    }

    fn charge_vec_swap(&mut self, ty: impl move_vm_types::views::TypeView) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        self.new.charge_vec_swap(ty.clone());
        self.current.charge_vec_swap(ty)
    }

    fn charge_load_resource(
        &mut self,
        loaded: Option<(
            move_core_types::gas_algebra::NumBytes,
            impl move_vm_types::views::ValueView,
        )>,
    ) -> PartialVMResult<()> {
        let loaded = loaded.map(|(x, y)| (x, MyVal::val(y)));
        self.new.charge_load_resource(loaded.clone());
        self.current.charge_load_resource(loaded.clone())
    }

    fn charge_native_function(
        &mut self,
        amount: InternalGas,
        ret_vals: Option<impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>>,
    ) -> PartialVMResult<()> {
        let ret_vals = ret_vals.map(|x| x.map(MyVal::val).collect::<Vec<_>>());
        self.new
            .charge_native_function(amount, ret_vals.clone().map(|x| x.into_iter()));
        self.current
            .charge_native_function(amount, ret_vals.map(|x| x.into_iter()))
    }

    fn charge_native_function_before_execution(
        &mut self,
        ty_args: impl ExactSizeIterator<Item = impl move_vm_types::views::TypeView>,
        args: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let ty_args: Vec<_> = ty_args.map(MyTag::val).collect();
        let args: Vec<_> = args.map(MyVal::val).collect();
        self.new.charge_native_function_before_execution(
            ty_args.clone().into_iter(),
            args.clone().into_iter(),
        );
        self.current
            .charge_native_function_before_execution(ty_args.into_iter(), args.into_iter())
    }

    fn charge_drop_frame(
        &mut self,
        locals: impl Iterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let locals: Vec<_> = locals.map(MyVal::val).collect();
        self.new.charge_drop_frame(locals.clone().into_iter());
        self.current.charge_drop_frame(locals.into_iter())
    }

    fn remaining_gas(&self) -> InternalGas {
        // NB: we need to call the trait method and not the struct method
        InternalGas::new(u64::from(GasMeter::remaining_gas(&self.current)))
    }
}

pub static INITIAL_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(T::initial_cost_schedule_v1);

pub fn initial_cost_schedule_for_unit_tests() -> move_vm_test_utils::gas_schedule::CostTable {
    T::initial_cost_schedule_for_unit_tests()
}
