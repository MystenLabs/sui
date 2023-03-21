// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::AbstractMemorySize;
use move_core_types::gas_algebra::GasQuantity;
use move_core_types::gas_algebra::InternalGas;
use move_core_types::gas_algebra::InternalGasUnit;
use move_core_types::gas_algebra::ToUnit;
use move_core_types::gas_algebra::ToUnitFractional;
use move_core_types::language_storage::TypeTag;
use move_vm_types::gas::GasMeter;
use move_vm_types::views::TypeView;
use move_vm_types::views::ValueView;
use move_vm_types::views::ValueVisitor;

use crate::double_units::CostTable;
use crate::double_units::Gas;
use crate::old_bytecode_tables as B;
use crate::old_units_types as BU;
use crate::tiered_tables as T;
use crate::tiered_units_types as TU;
use once_cell::sync::Lazy;

#[derive(Debug)]
pub struct GasStatus<'a> {
    pub bytecode: B::GasStatus<'a>,
    pub tiered: T::GasStatus<'a>,
    start_time: Instant,
}

impl<'a> GasStatus<'a> {
    pub fn new(cost_table: &'a CostTable, gas_left: Gas) -> Self {
        Self {
            bytecode: B::GasStatus::new(&cost_table.bytecode, BU::Gas::new(u64::from(gas_left))),
            //tiered: T::GasStatus::new(&cost_table.tiers, TU::Gas::new(u64::from(gas_left))),
            // Tiered _never_ runs out of gas here, so start it at u64::MAX
            tiered: T::GasStatus::new(&cost_table.tiers, TU::Gas::new(u32::MAX as u64)),
            start_time: Instant::now(),
        }
    }

    pub fn new_unmetered() -> Self {
        Self {
            bytecode: B::GasStatus::new_unmetered(),
            tiered: T::GasStatus::new_unmetered(),
            start_time: Instant::now(),
        }
    }

    pub fn remaining_gas(&self) -> Gas {
        Gas::new(u64::from(self.bytecode.remaining_gas()))
    }

    pub fn set_metering(&mut self, enabled: bool) {
        self.bytecode.set_metering(enabled);
        self.tiered.set_metering(enabled);
    }

    pub fn deduct_gas(&mut self, amount: InternalGas) -> PartialVMResult<()> {
        self.bytecode.deduct_gas(amount)?;
        self.tiered.deduct_gas(amount)?;
        Ok(())
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

impl<'a> GasMeter for GasStatus<'a> {
    fn charge_simple_instr(
        &mut self,
        instr: move_vm_types::gas::SimpleInstruction,
    ) -> PartialVMResult<()> {
        self.tiered.charge_simple_instr(instr);
        self.bytecode.charge_simple_instr(instr)
    }

    fn charge_pop(
        &mut self,
        popped_val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        self.tiered.charge_pop(MyVal::ref_(&popped_val));
        self.bytecode.charge_pop(MyVal::ref_(&popped_val))
    }

    fn charge_call(
        &mut self,
        module_id: &move_core_types::language_storage::ModuleId,
        func_name: &str,
        args: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
        num_locals: move_core_types::gas_algebra::NumArgs,
    ) -> PartialVMResult<()> {
        let args: Vec<_> = args.map(MyVal::val).collect();
        self.tiered
            .charge_call(module_id, func_name, args.clone().into_iter(), num_locals);
        self.bytecode
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
        self.tiered.charge_call_generic(
            module_id,
            func_name,
            ty_args.clone().into_iter(),
            args.clone().into_iter(),
            num_locals,
        );
        self.bytecode.charge_call_generic(
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
        self.tiered.charge_ld_const(size);
        self.bytecode.charge_ld_const(size)
    }

    fn charge_ld_const_after_deserialization(
        &mut self,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let val = MyVal::val(val);
        self.tiered.charge_ld_const_after_deserialization(val);
        self.bytecode.charge_ld_const_after_deserialization(val)
    }

    fn charge_copy_loc(
        &mut self,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let val = MyVal::val(val);
        self.tiered.charge_copy_loc(val);
        self.bytecode.charge_copy_loc(val)
    }

    fn charge_move_loc(
        &mut self,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let val = MyVal::val(val);
        self.tiered.charge_move_loc(val);
        self.bytecode.charge_move_loc(val)
    }

    fn charge_store_loc(
        &mut self,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let val = MyVal::val(val);
        self.tiered.charge_store_loc(val);
        self.bytecode.charge_store_loc(val)
    }

    fn charge_pack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let args: Vec<_> = args.map(MyVal::val).collect();
        self.tiered
            .charge_pack(is_generic, args.clone().into_iter());
        self.bytecode.charge_pack(is_generic, args.into_iter())
    }

    fn charge_unpack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let args: Vec<_> = args.map(MyVal::val).collect();
        self.tiered
            .charge_unpack(is_generic, args.clone().into_iter());
        self.bytecode.charge_unpack(is_generic, args.into_iter())
    }

    fn charge_read_ref(
        &mut self,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let val = MyVal::val(val);
        self.tiered.charge_read_ref(val);
        self.bytecode.charge_read_ref(val)
    }

    fn charge_write_ref(
        &mut self,
        new_val: impl move_vm_types::views::ValueView,
        old_val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let new_val = MyVal::val(new_val);
        let old_val = MyVal::val(old_val);
        self.tiered.charge_write_ref(new_val, old_val);
        self.bytecode.charge_write_ref(new_val, old_val)
    }

    fn charge_eq(
        &mut self,
        lhs: impl move_vm_types::views::ValueView,
        rhs: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let lhs = MyVal::val(lhs);
        let rhs = MyVal::val(rhs);
        self.tiered.charge_eq(lhs, rhs);
        self.bytecode.charge_eq(lhs, rhs)
    }

    fn charge_neq(
        &mut self,
        lhs: impl move_vm_types::views::ValueView,
        rhs: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let lhs = MyVal::val(lhs);
        let rhs = MyVal::val(rhs);
        self.tiered.charge_neq(lhs, rhs);
        self.bytecode.charge_neq(lhs, rhs)
    }

    fn charge_borrow_global(
        &mut self,
        is_mut: bool,
        is_generic: bool,
        ty: impl move_vm_types::views::TypeView,
        is_success: bool,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        self.tiered
            .charge_borrow_global(is_mut, is_generic, ty.clone(), is_success);
        self.bytecode
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
        self.bytecode.charge_exists(is_generic, ty.clone(), exists);
        self.tiered.charge_exists(is_generic, ty, exists)
    }

    fn charge_move_from(
        &mut self,
        is_generic: bool,
        ty: impl move_vm_types::views::TypeView,
        val: Option<impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        let val = val.map(MyVal::val);
        self.bytecode
            .charge_move_from(is_generic, ty.clone(), val.clone());
        self.tiered.charge_move_from(is_generic, ty, val)
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
        self.tiered
            .charge_move_to(is_generic, ty.clone(), val.clone(), is_success);
        self.bytecode
            .charge_move_to(is_generic, ty, val, is_success)
    }

    fn charge_vec_pack<'b>(
        &mut self,
        ty: impl move_vm_types::views::TypeView + 'b,
        args: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        let args: Vec<_> = args.map(MyVal::val).collect();
        self.tiered
            .charge_vec_pack(ty.clone(), args.clone().into_iter());
        self.bytecode.charge_vec_pack(ty, args.into_iter())
    }

    fn charge_vec_len(&mut self, ty: impl move_vm_types::views::TypeView) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        self.tiered.charge_vec_len(ty.clone());
        self.bytecode.charge_vec_len(ty)
    }

    fn charge_vec_borrow(
        &mut self,
        is_mut: bool,
        ty: impl move_vm_types::views::TypeView,
        is_success: bool,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        self.tiered
            .charge_vec_borrow(is_mut, ty.clone(), is_success);
        self.bytecode.charge_vec_borrow(is_mut, ty, is_success)
    }

    fn charge_vec_push_back(
        &mut self,
        ty: impl move_vm_types::views::TypeView,
        val: impl move_vm_types::views::ValueView,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        let val = MyVal::val(val);
        self.tiered.charge_vec_push_back(ty.clone(), val.clone());
        self.bytecode.charge_vec_push_back(ty, val)
    }

    fn charge_vec_pop_back(
        &mut self,
        ty: impl move_vm_types::views::TypeView,
        val: Option<impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        let val = val.map(MyVal::val);
        self.tiered.charge_vec_pop_back(ty.clone(), val.clone());
        self.bytecode.charge_vec_pop_back(ty, val)
    }

    fn charge_vec_unpack(
        &mut self,
        ty: impl move_vm_types::views::TypeView,
        expect_num_elements: move_core_types::gas_algebra::NumArgs,
        elems: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        let elems: Vec<_> = elems.map(MyVal::val).collect();
        self.tiered
            .charge_vec_unpack(ty.clone(), expect_num_elements, elems.clone().into_iter());
        self.bytecode
            .charge_vec_unpack(ty, expect_num_elements, elems.into_iter())
    }

    fn charge_vec_swap(&mut self, ty: impl move_vm_types::views::TypeView) -> PartialVMResult<()> {
        let ty = MyTag::val(ty);
        self.tiered.charge_vec_swap(ty.clone());
        self.bytecode.charge_vec_swap(ty)
    }

    fn charge_load_resource(
        &mut self,
        loaded: Option<(
            move_core_types::gas_algebra::NumBytes,
            impl move_vm_types::views::ValueView,
        )>,
    ) -> PartialVMResult<()> {
        let loaded = loaded.map(|(x, y)| (x, MyVal::val(y)));
        self.tiered.charge_load_resource(loaded.clone());
        self.bytecode.charge_load_resource(loaded.clone())
    }

    fn charge_native_function(
        &mut self,
        amount: InternalGas,
        ret_vals: Option<impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>>,
    ) -> PartialVMResult<()> {
        let ret_vals = ret_vals.map(|x| x.map(MyVal::val).collect::<Vec<_>>());
        self.tiered
            .charge_native_function(amount, ret_vals.clone().map(|x| x.into_iter()));
        self.bytecode
            .charge_native_function(amount, ret_vals.map(|x| x.into_iter()))
    }

    fn charge_native_function_before_execution(
        &mut self,
        ty_args: impl ExactSizeIterator<Item = impl move_vm_types::views::TypeView>,
        args: impl ExactSizeIterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let ty_args: Vec<_> = ty_args.map(MyTag::val).collect();
        let args: Vec<_> = args.map(MyVal::val).collect();
        self.tiered.charge_native_function_before_execution(
            ty_args.clone().into_iter(),
            args.clone().into_iter(),
        );
        self.bytecode
            .charge_native_function_before_execution(ty_args.into_iter(), args.into_iter())
    }

    fn charge_drop_frame(
        &mut self,
        locals: impl Iterator<Item = impl move_vm_types::views::ValueView>,
    ) -> PartialVMResult<()> {
        let locals: Vec<_> = locals.map(MyVal::val).collect();
        self.tiered.charge_drop_frame(locals.clone().into_iter());
        self.bytecode.charge_drop_frame(locals.into_iter())
    }

    fn remaining_gas(&self) -> InternalGas {
        // NB: we need to call the trait method and not the struct method
        InternalGas::new(u64::from(GasMeter::remaining_gas(&self.bytecode)))
    }
}

pub static INITIAL_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(|| CostTable {
    bytecode: B::INITIAL_COST_SCHEDULE.clone(),
    tiers: T::INITIAL_COST_SCHEDULE.clone(),
});

pub fn initial_cost_schedule_for_unit_tests() -> move_vm_test_utils::gas_schedule::CostTable {
    T::initial_cost_schedule_for_unit_tests()
}
