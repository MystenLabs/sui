// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module lays out the basic abstract costing schedule for bytecode instructions.
//!
//! It is important to note that the cost schedule defined in this file does not track hashing
//! operations or other native operations; the cost of each native operation will be returned by the
//! native function itself.
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    gas_algebra::{
        AbstractMemorySize, GasQuantity, InternalGas, InternalGasUnit, NumArgs, NumBytes, ToUnit,
        ToUnitFractional,
    },
    language_storage::ModuleId,
    vm_status::StatusCode,
};
use move_vm_types::{
    gas::{GasMeter, SimpleInstruction},
    loaded_data::runtime_types::Type,
    views::{TypeView, ValueView},
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ops::{Add, Bound},
    u64,
};

pub enum GasUnit {}

pub type Gas = GasQuantity<GasUnit>;

impl ToUnit<InternalGasUnit> for GasUnit {
    const MULTIPLIER: u64 = 1000;
}

impl ToUnitFractional<GasUnit> for InternalGasUnit {
    const NOMINATOR: u64 = 1;
    const DENOMINATOR: u64 = 1000;
}

/// The size in bytes for a non-string or address constant on the stack
pub const CONST_SIZE: AbstractMemorySize = AbstractMemorySize::new(16);

/// The size in bytes for a reference on the stack
pub const REFERENCE_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);

/// The size of a struct in bytes
pub const STRUCT_SIZE: AbstractMemorySize = AbstractMemorySize::new(2);

/// The size of a vector (without its containing data) in bytes
pub const VEC_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);

/// For exists checks on data that doesn't exists this is the multiplier that is used.
pub const MIN_EXISTS_DATA_SIZE: AbstractMemorySize = AbstractMemorySize::new(100);

pub const INSTRUCTION_TIER_DEFAULT: u64 = 1;

pub const STACK_HEIGHT_TIER_DEFAULT: u64 = 1;
pub const STACK_SIZE_TIER_DEFAULT: u64 = 1;

// The cost table holds the tiers and curves for instruction costs.
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Deserialize)]
pub struct CostTable {
    pub instruction_tiers: BTreeMap<u64, u64>,
    pub stack_height_tiers: BTreeMap<u64, u64>,
    pub stack_size_tiers: BTreeMap<u64, u64>,
}

impl CostTable {
    fn get_current_and_future_tier(
        tiers: &BTreeMap<u64, u64>,
        current: u64,
        default: u64,
    ) -> (u64, Option<u64>) {
        let current_cost = tiers
            .get(&current)
            .or_else(|| tiers.range(..current).next_back().map(|(_, v)| v))
            .unwrap_or(&default);
        let next_tier_start = tiers
            .range::<u64, _>((Bound::Excluded(current), Bound::Unbounded))
            .next()
            .map(|(next_tier_start, _)| *next_tier_start);
        (*current_cost, next_tier_start)
    }

    pub fn instruction_tier(&self, instr_count: u64) -> (u64, Option<u64>) {
        Self::get_current_and_future_tier(
            &self.instruction_tiers,
            instr_count,
            INSTRUCTION_TIER_DEFAULT,
        )
    }

    pub fn stack_height_tier(&self, stack_height: u64) -> (u64, Option<u64>) {
        Self::get_current_and_future_tier(
            &self.stack_height_tiers,
            stack_height,
            STACK_HEIGHT_TIER_DEFAULT,
        )
    }

    pub fn stack_size_tier(&self, stack_size: u64) -> (u64, Option<u64>) {
        Self::get_current_and_future_tier(
            &self.stack_size_tiers,
            stack_size,
            STACK_SIZE_TIER_DEFAULT,
        )
    }
}

/// The  `GasCost` tracks:
/// - instruction cost: how much time/computational power is needed to perform the instruction
/// - memory cost: how much memory is required for the instruction, and storage overhead
/// - stack height: how high is the stack growing (regardless of size in bytes)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GasCost {
    pub instruction_gas: u64,
    pub memory_gas: u64,
    pub stack_height_gas: u64,
}

impl GasCost {
    pub fn new(instruction_gas: u64, memory_gas: u64, stack_height_gas: u64) -> Self {
        Self {
            instruction_gas,
            memory_gas,
            stack_height_gas,
        }
    }

    /// Convert a GasCost to a total gas charge in `InternalGas`.
    #[inline]
    pub fn total(&self) -> u64 {
        self.instruction_gas
            .add(self.memory_gas)
            .add(self.stack_height_gas)
    }

    #[inline]
    pub fn total_internal(&self) -> InternalGas {
        GasQuantity::new(
            self.instruction_gas
                .add(self.memory_gas)
                .add(self.stack_height_gas),
        )
    }
}

/// The Move VM implementation of state for gas metering.
///
/// Initialize with a `CostTable` and the gas provided to the transaction.
/// Provide all the proper guarantees about gas metering in the Move VM.
///
/// Every client must use an instance of this type to interact with the Move VM.
pub struct GasStatus<'a> {
    cost_table: &'a CostTable,
    gas_left: InternalGas,
    charge: bool,

    // The current height of the operand stack, and the maximal height that it has reached.
    stack_height_high_water_mark: u64,
    stack_height_current: u64,
    stack_height_next_tier_start: Option<u64>,
    stack_height_current_tier_mult: u64,

    // The current (abstract) size  of the operand stack and the maximal size that it has reached.
    stack_size_high_water_mark: u64,
    stack_size_current: u64,
    stack_size_next_tier_start: Option<u64>,
    stack_size_current_tier_mult: u64,

    // The total number of bytecode instructions that have been executed in the transaction.
    instructions_executed: u64,
    instructions_next_tier_start: Option<u64>,
    instructions_current_tier_mult: u64,
}

impl<'a> GasStatus<'a> {
    /// Initialize the gas state with metering enabled.
    ///
    /// Charge for every operation and fail when there is no more gas to pay for operations.
    /// This is the instantiation that must be used when executing a user script.
    pub fn new(cost_table: &'a CostTable, gas_left: Gas) -> Self {
        let (stack_height_current_tier_mult, stack_height_next_tier_start) =
            cost_table.stack_height_tier(0);
        let (stack_size_current_tier_mult, stack_size_next_tier_start) =
            cost_table.stack_size_tier(0);
        let (instructions_current_tier_mult, instructions_next_tier_start) =
            cost_table.instruction_tier(0);
        Self {
            gas_left: gas_left.to_unit(),
            cost_table,
            charge: true,
            stack_height_high_water_mark: 0,
            stack_height_current: 0,
            stack_size_high_water_mark: 0,
            stack_size_current: 0,
            instructions_executed: 0,
            stack_height_current_tier_mult,
            stack_size_current_tier_mult,
            instructions_current_tier_mult,
            stack_height_next_tier_start,
            stack_size_next_tier_start,
            instructions_next_tier_start,
        }
    }

    /// Initialize the gas state with metering disabled.
    ///
    /// It should be used by clients in very specific cases and when executing system
    /// code that does not have to charge the user.
    pub fn new_unmetered() -> Self {
        Self {
            gas_left: InternalGas::new(0),
            cost_table: &ZERO_COST_SCHEDULE,
            charge: false,
            stack_height_high_water_mark: 0,
            stack_height_current: 0,
            stack_size_high_water_mark: 0,
            stack_size_current: 0,
            instructions_executed: 0,
            stack_height_current_tier_mult: 0,
            stack_size_current_tier_mult: 0,
            instructions_current_tier_mult: 0,
            stack_height_next_tier_start: None,
            stack_size_next_tier_start: None,
            instructions_next_tier_start: None,
        }
    }

    pub fn push_stack(&mut self, pushes: u64) -> PartialVMResult<()> {
        match self.stack_height_current.checked_add(pushes) {
            // We should never hit this.
            None => return Err(PartialVMError::new(StatusCode::ARITHMETIC_OVERFLOW)),
            Some(new_height) => {
                if new_height > self.stack_height_high_water_mark {
                    self.stack_height_high_water_mark = new_height;
                }
                self.stack_height_current = new_height;
            }
        }

        if let Some(stack_height_tier_next) = self.stack_height_next_tier_start {
            if self.stack_height_current > stack_height_tier_next {
                let (next_mul, next_tier) =
                    self.cost_table.stack_height_tier(self.stack_height_current);
                self.stack_height_current_tier_mult = next_mul;
                self.stack_height_next_tier_start = next_tier;
            }
        }

        Ok(())
    }

    pub fn pop_stack(&mut self, pops: u64) {
        self.stack_height_current = self.stack_height_current.saturating_sub(pops);
    }

    pub fn increase_instruction_count(&mut self, amount: u64) -> PartialVMResult<()> {
        match self.instructions_executed.checked_add(amount) {
            None => return Err(PartialVMError::new(StatusCode::PC_OVERFLOW)),
            Some(new_pc) => {
                self.instructions_executed = new_pc;
            }
        }

        if let Some(instr_tier_next) = self.instructions_next_tier_start {
            if self.instructions_executed > instr_tier_next {
                let (instr_cost, next_tier) =
                    self.cost_table.instruction_tier(self.instructions_executed);
                self.instructions_current_tier_mult = instr_cost;
                self.instructions_next_tier_start = next_tier;
            }
        }

        Ok(())
    }

    pub fn increase_stack_size(&mut self, size_amount: u64) -> PartialVMResult<()> {
        match self.stack_size_current.checked_add(size_amount) {
            None => return Err(PartialVMError::new(StatusCode::ARITHMETIC_OVERFLOW)),
            Some(new_size) => {
                if new_size > self.stack_size_high_water_mark {
                    self.stack_size_high_water_mark = new_size;
                }
                self.stack_size_current = new_size;
            }
        }

        if let Some(stack_size_tier_next) = self.stack_size_next_tier_start {
            if self.stack_size_current > stack_size_tier_next {
                let (next_mul, next_tier) =
                    self.cost_table.stack_size_tier(self.stack_size_current);
                self.stack_size_current_tier_mult = next_mul;
                self.stack_size_next_tier_start = next_tier;
            }
        }

        Ok(())
    }

    pub fn decrease_stack_size(&mut self, size_amount: u64) {
        let new_size = self.stack_size_current.saturating_sub(size_amount);
        if new_size > self.stack_size_high_water_mark {
            self.stack_size_high_water_mark = new_size;
        }
        self.stack_size_current = new_size;
    }

    /// Given: pushes + pops + increase + decrease in size for an instruction charge for the
    /// execution of the instruction.
    pub fn charge(
        &mut self,
        num_instructions: u64,
        pushes: u64,
        pops: u64,
        incr_size: u64,
        _decr_size: u64,
    ) -> PartialVMResult<()> {
        self.push_stack(pushes)?;
        self.increase_instruction_count(num_instructions)?;
        self.increase_stack_size(incr_size)?;

        self.deduct_gas(
            GasCost::new(
                self.instructions_current_tier_mult
                    .checked_mul(num_instructions)
                    .ok_or_else(|| PartialVMError::new(StatusCode::ARITHMETIC_OVERFLOW))?,
                self.stack_size_current_tier_mult
                    .checked_mul(incr_size)
                    .ok_or_else(|| PartialVMError::new(StatusCode::ARITHMETIC_OVERFLOW))?,
                self.stack_height_current_tier_mult
                    .checked_mul(pushes)
                    .ok_or_else(|| PartialVMError::new(StatusCode::ARITHMETIC_OVERFLOW))?,
            )
            .total_internal(),
        )?;

        // self.decrease_stack_size(decr_size);
        self.pop_stack(pops);
        Ok(())
    }

    /// Return the `CostTable` behind this `GasStatus`.
    pub fn cost_table(&self) -> &CostTable {
        self.cost_table
    }

    /// Return the gas left.
    pub fn remaining_gas(&self) -> Gas {
        self.gas_left.to_unit_round_down()
    }

    /// Charge a given amount of gas and fail if not enough gas units are left.
    pub fn deduct_gas(&mut self, amount: InternalGas) -> PartialVMResult<()> {
        if !self.charge {
            return Ok(());
        }

        match self.gas_left.checked_sub(amount) {
            Some(gas_left) => {
                self.gas_left = gas_left;
                Ok(())
            }
            None => {
                self.gas_left = InternalGas::new(0);
                Err(PartialVMError::new(StatusCode::OUT_OF_GAS))
            }
        }
    }

    pub fn set_metering(&mut self, enabled: bool) {
        self.charge = enabled
    }
}

/// Returns a tuple of (<pops>, <pushes>, <stack_size_decrease>, <stack_size_increase>)
fn get_simple_instruction_stack_change(
    instr: SimpleInstruction,
) -> (u64, u64, AbstractMemorySize, AbstractMemorySize) {
    use SimpleInstruction::*;

    match instr {
        // NB: The `Ret` pops are accounted for in `Call` instructions, so we say `Ret` has no pops.
        Nop | Ret => (0, 0, 0.into(), 0.into()),
        BrTrue | BrFalse => (1, 0, Type::Bool.size(), 0.into()),
        Branch => (0, 0, 0.into(), 0.into()),
        LdU8 => (0, 1, 0.into(), Type::U8.size()),
        LdU16 => (0, 1, 0.into(), Type::U16.size()),
        LdU32 => (0, 1, 0.into(), Type::U32.size()),
        LdU64 => (0, 1, 0.into(), Type::U64.size()),
        LdU128 => (0, 1, 0.into(), Type::U128.size()),
        LdU256 => (0, 1, 0.into(), Type::U256.size()),
        LdTrue | LdFalse => (0, 1, 0.into(), Type::Bool.size()),
        FreezeRef => (1, 1, REFERENCE_SIZE, REFERENCE_SIZE),
        ImmBorrowLoc | MutBorrowLoc => (0, 1, 0.into(), REFERENCE_SIZE),
        ImmBorrowField | MutBorrowField | ImmBorrowFieldGeneric | MutBorrowFieldGeneric => {
            (1, 1, REFERENCE_SIZE, REFERENCE_SIZE)
        }
        // Since we don't have the size of the value being cast here we take a conservative
        // over-approximation: it is _always_ getting cast from the smallest integer type.
        CastU8 => (1, 1, Type::U8.size(), Type::U8.size()),
        CastU16 => (1, 1, Type::U8.size(), Type::U16.size()),
        CastU32 => (1, 1, Type::U8.size(), Type::U32.size()),
        CastU64 => (1, 1, Type::U8.size(), Type::U64.size()),
        CastU128 => (1, 1, Type::U8.size(), Type::U128.size()),
        CastU256 => (1, 1, Type::U8.size(), Type::U256.size()),
        // NB: We don't know the size of what integers we're dealing with, so we conservatively
        // over-approximate by popping the smallest integers, and push the largest.
        Add | Sub | Mul | Mod | Div => (2, 1, Type::U8.size() + Type::U8.size(), Type::U256.size()),
        BitOr | BitAnd | Xor => (2, 1, Type::U8.size() + Type::U8.size(), Type::U256.size()),
        Shl | Shr => (2, 1, Type::U8.size() + Type::U8.size(), Type::U256.size()),
        Or | And => (
            2,
            1,
            Type::Bool.size() + Type::Bool.size(),
            Type::Bool.size(),
        ),
        Lt | Gt | Le | Ge => (2, 1, Type::U8.size() + Type::U8.size(), Type::Bool.size()),
        Not => (1, 1, Type::Bool.size(), Type::Bool.size()),
        Abort => (1, 0, Type::U64.size(), 0.into()),
    }
}

impl<'b> GasMeter for GasStatus<'b> {
    /// Charge an instruction and fail if not enough gas units are left.
    fn charge_simple_instr(&mut self, instr: SimpleInstruction) -> PartialVMResult<()> {
        let (pops, pushes, pop_size, push_size) = get_simple_instruction_stack_change(instr);
        self.charge(1, pushes, pops, push_size.into(), pop_size.into())
    }

    fn charge_pop(&mut self, popped_val: impl ValueView) -> PartialVMResult<()> {
        self.charge(1, 0, 1, 0, popped_val.legacy_abstract_memory_size().into())
    }

    fn charge_native_function(
        &mut self,
        amount: InternalGas,
        ret_vals: Option<impl ExactSizeIterator<Item = impl ValueView>>,
    ) -> PartialVMResult<()> {
        // Charge for the number of pushes on to the stack that the return of this function is
        // going to cause.
        let pushes = ret_vals
            .as_ref()
            .map(|ret_vals| ret_vals.len())
            .unwrap_or(0) as u64;
        // Calculate the number of bytes that are getting pushed onto the stack.
        let size_increase = ret_vals
            .map(|ret_vals| {
                ret_vals.fold(AbstractMemorySize::zero(), |acc, elem| {
                    acc + elem.legacy_abstract_memory_size()
                })
            })
            .unwrap_or_else(AbstractMemorySize::zero);
        // Charge for the stack operations. We don't count this as an "instruction" since we
        // already accounted for the `Call` instruction in the
        // `charge_native_function_before_execution` call.
        self.charge(0, pushes, 0, size_increase.into(), 0)?;
        // Now charge the gas that the native function told us to charge.
        self.deduct_gas(amount)
    }

    fn charge_native_function_before_execution(
        &mut self,
        _ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        // Determine the number of pops that are going to be needed for this function call, and
        // charge for them.
        let pops = args.len() as u64;
        // Calculate the size decrease of the stack from the above pops.
        let stack_reduction_size = args.fold(AbstractMemorySize::new(pops), |acc, elem| {
            acc + elem.legacy_abstract_memory_size()
        });
        // Track that this is going to be popping from the operand stack. We also increment the
        // instruction count as we need to account for the `Call` bytecode that initiated this
        // native call.
        self.charge(1, 0, pops, 0, stack_reduction_size.into())
    }

    fn charge_call(
        &mut self,
        _module_id: &ModuleId,
        _func_name: &str,
        args: impl ExactSizeIterator<Item = impl ValueView>,
        _num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        // We will have to perform this many pops for the call.
        let pops = args.len() as u64;
        // Size stays the same -- we're just moving it from the operand stack to the locals. But
        // the size on the operand stack is reduced by sum_{args} arg.size().
        let stack_reduction_size = args.fold(AbstractMemorySize::new(0), |acc, elem| {
            acc + elem.legacy_abstract_memory_size()
        });
        self.charge(1, 0, pops, 0, stack_reduction_size.into())
    }

    fn charge_call_generic(
        &mut self,
        _module_id: &ModuleId,
        _func_name: &str,
        _ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        args: impl ExactSizeIterator<Item = impl ValueView>,
        _num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        // We have to perform this many pops from the operand stack for this function call.
        let pops = args.len() as u64;
        // Calculate the size reduction on the operand stack.
        let stack_reduction_size = args.fold(AbstractMemorySize::new(0), |acc, elem| {
            acc + elem.legacy_abstract_memory_size()
        });
        // Charge for the pops, no pushes, and account for the stack size decrease. Also track the
        // `CallGeneric` instruction we must have encountered for this.
        self.charge(1, 0, pops, 0, stack_reduction_size.into())
    }

    fn charge_ld_const(&mut self, size: NumBytes) -> PartialVMResult<()> {
        // Charge for the load from the locals onto the stack.
        self.charge(1, 1, 0, u64::from(size), 0)
    }

    fn charge_ld_const_after_deserialization(
        &mut self,
        _val: impl ValueView,
    ) -> PartialVMResult<()> {
        // We already charged for this based on the bytes that we're loading so don't charge again.
        Ok(())
    }

    fn charge_copy_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        // Charge for the copy of the local onto the stack.
        self.charge(1, 1, 0, val.legacy_abstract_memory_size().into(), 0)
    }

    fn charge_move_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        // Charge for the move of the local on to the stack. Note that we charge here since we
        // aren't tracking the local size (at least not yet). If we were, this should be a net-zero
        // operation in terms of memory usage.
        self.charge(1, 1, 0, val.legacy_abstract_memory_size().into(), 0)
    }

    fn charge_store_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        // Charge for the storing of the value on the stack into a local. Note here that if we were
        // also accounting for the size of the locals that this would be a net-zero operation in
        // terms of memory.
        self.charge(1, 0, 1, 0, val.legacy_abstract_memory_size().into())
    }

    fn charge_pack(
        &mut self,
        _is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        // We perform `num_fields` number of pops.
        let num_fields = args.len() as u64;
        // The actual amount of memory on the stack is staying the same with the addition of some
        // extra size for the struct, so the size doesn't really change much.
        self.charge(1, 1, num_fields, STRUCT_SIZE.into(), 0)
    }

    fn charge_unpack(
        &mut self,
        _is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        // We perform `num_fields` number of pushes.
        let num_fields = args.len() as u64;
        self.charge(1, num_fields, 1, 0, STRUCT_SIZE.into())
    }

    fn charge_read_ref(&mut self, ref_val: impl ValueView) -> PartialVMResult<()> {
        // We read the the reference so we are decreasing the size of the stack by the size of the
        // reference, and adding to it the size of the value that has been read from that
        // reference.
        self.charge(
            1,
            1,
            1,
            ref_val.legacy_abstract_memory_size().into(),
            REFERENCE_SIZE.into(),
        )
    }

    fn charge_write_ref(
        &mut self,
        new_val: impl ValueView,
        old_val: impl ValueView,
    ) -> PartialVMResult<()> {
        // TODO(tzakian): We should account for this elsewhere as the owner of data the the
        // reference points to won't be on the stack. For now though, we treat it as adding to the
        // stack size.
        self.charge(
            1,
            1,
            2,
            new_val.legacy_abstract_memory_size().into(),
            old_val.legacy_abstract_memory_size().into(),
        )
    }

    fn charge_eq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        let size_reduction = lhs.legacy_abstract_memory_size() + rhs.legacy_abstract_memory_size();
        self.charge(
            1,
            1,
            2,
            (Type::Bool.size() + size_reduction).into(),
            size_reduction.into(),
        )
    }

    fn charge_neq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        let size_reduction = lhs.legacy_abstract_memory_size() + rhs.legacy_abstract_memory_size();
        self.charge(1, 1, 2, Type::Bool.size().into(), size_reduction.into())
    }

    fn charge_load_resource(
        &mut self,
        _loaded: Option<(NumBytes, impl ValueView)>,
    ) -> PartialVMResult<()> {
        // We don't have resource loading so don't need to account for it.
        Ok(())
    }

    fn charge_borrow_global(
        &mut self,
        _is_mut: bool,
        _is_generic: bool,
        _ty: impl TypeView,
        _is_success: bool,
    ) -> PartialVMResult<()> {
        self.charge(1, 1, 1, REFERENCE_SIZE.into(), Type::Address.size().into())
    }

    fn charge_exists(
        &mut self,
        _is_generic: bool,
        _ty: impl TypeView,
        // TODO(Gas): see if we can get rid of this param
        _exists: bool,
    ) -> PartialVMResult<()> {
        self.charge(
            1,
            1,
            1,
            Type::Bool.size().into(),
            Type::Address.size().into(),
        )
    }

    fn charge_move_from(
        &mut self,
        _is_generic: bool,
        ty: impl TypeView,
        val: Option<impl ValueView>,
    ) -> PartialVMResult<()> {
        let size = val
            .map(|val| val.legacy_abstract_memory_size())
            .unwrap_or_else(|| ty.to_type_tag().abstract_size_for_gas_metering());
        self.charge(1, 1, 1, size.into(), Type::Address.size().into())
    }

    fn charge_move_to(
        &mut self,
        _is_generic: bool,
        _ty: impl TypeView,
        _val: impl ValueView,
        _is_success: bool,
    ) -> PartialVMResult<()> {
        self.charge(1, 0, 2, 0, Type::Address.size().into())
    }

    fn charge_vec_pack<'a>(
        &mut self,
        _ty: impl TypeView + 'a,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        // We will perform `num_args` number of pops.
        let num_args = args.len() as u64;
        // The amount of data on the stack stays contstant except we have some extra metadata for
        // the vector to hold the length of the vector.
        self.charge(1, 1, num_args, VEC_SIZE.into(), 0)
    }

    fn charge_vec_len(&mut self, _ty: impl TypeView) -> PartialVMResult<()> {
        self.charge(1, 1, 1, Type::U64.size().into(), REFERENCE_SIZE.into())
    }

    fn charge_vec_borrow(
        &mut self,
        _is_mut: bool,
        _ty: impl TypeView,
        _is_success: bool,
    ) -> PartialVMResult<()> {
        self.charge(
            1,
            1,
            2,
            REFERENCE_SIZE.into(),
            (REFERENCE_SIZE + Type::U64.size()).into(),
        )
    }

    fn charge_vec_push_back(
        &mut self,
        _ty: impl TypeView,
        _val: impl ValueView,
    ) -> PartialVMResult<()> {
        // The value was already on the stack, so we aren't increasing the number of bytes on the stack.
        self.charge(1, 0, 2, 0, REFERENCE_SIZE.into())
    }

    fn charge_vec_pop_back(
        &mut self,
        _ty: impl TypeView,
        _val: Option<impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge(1, 1, 1, 0, REFERENCE_SIZE.into())
    }

    fn charge_vec_unpack(
        &mut self,
        _ty: impl TypeView,
        expect_num_elements: NumArgs,
        _elems: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        // Charge for the pushes
        let pushes = u64::from(expect_num_elements);
        // The stack size stays pretty much the same modulo the additional vector size
        self.charge(1, pushes, 1, 0, VEC_SIZE.into())
    }

    fn charge_vec_swap(&mut self, _ty: impl TypeView) -> PartialVMResult<()> {
        let size_decrease = REFERENCE_SIZE + Type::U64.size() + Type::U64.size();
        self.charge(1, 1, 1, 0, size_decrease.into())
    }

    fn charge_drop_frame(
        &mut self,
        _locals: impl Iterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn remaining_gas(&self) -> InternalGas {
        self.gas_left
    }
}

pub fn zero_cost_schedule() -> CostTable {
    let mut zero_tier = BTreeMap::new();
    zero_tier.insert(0, 0);
    CostTable {
        instruction_tiers: zero_tier.clone(),
        stack_size_tiers: zero_tier.clone(),
        stack_height_tiers: zero_tier,
    }
}

pub fn unit_cost_schedule() -> CostTable {
    let mut unit_tier = BTreeMap::new();
    unit_tier.insert(0, 1);
    CostTable {
        instruction_tiers: unit_tier.clone(),
        stack_size_tiers: unit_tier.clone(),
        stack_height_tiers: unit_tier,
    }
}

pub fn initial_cost_schedule() -> CostTable {
    let instruction_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (1000, 2),
        (2000, 4),
        (2500, 16),
        (3000, 256), // After this all instructions get charged this amount.
    ]
    .into_iter()
    .collect();

    let stack_height_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (200, 2),
        (400, 4),
        (800, 16),
        (1200, 256), // After this all increases to the stack height get charged this amount.
    ]
    .into_iter()
    .collect();

    let stack_size_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (5000, 2),
        (7000, 4),
        (10000, 16),
        (15000, 256), // After this all increases to the stack height get charged this amount.
    ]
    .into_iter()
    .collect();

    CostTable {
        instruction_tiers,
        stack_size_tiers,
        stack_height_tiers,
    }
}

static ZERO_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(zero_cost_schedule);

pub static INITIAL_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(initial_cost_schedule);
