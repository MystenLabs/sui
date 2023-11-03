// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_binary_format::errors::{PartialVMError, PartialVMResult};

use move_core_types::gas_algebra::{AbstractMemorySize, InternalGas, NumArgs, NumBytes};
use move_core_types::language_storage::ModuleId;

use move_core_types::vm_status::StatusCode;
#[cfg(feature = "gas-profiler")]
use move_vm_profiler::GasProfiler;
use move_vm_types::gas::{GasMeter, SimpleInstruction};
use move_vm_types::loaded_data::runtime_types::Type;
use move_vm_types::views::{TypeView, ValueView};
use once_cell::sync::Lazy;

use crate::gas_model::units_types::{CostTable, Gas, GasCost};

use super::gas_predicates::charge_input_as_memory;
use super::gas_predicates::use_legacy_abstract_size;

/// VM flat fee
pub const VM_FLAT_FEE: Gas = Gas::new(8_000);

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

pub static ZERO_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(zero_cost_schedule);

pub static INITIAL_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(initial_cost_schedule_v1);

/// The Move VM implementation of state for gas metering.
///
/// Initialize with a `CostTable` and the gas provided to the transaction.
/// Provide all the proper guarantees about gas metering in the Move VM.
///
/// Every client must use an instance of this type to interact with the Move VM.
#[allow(dead_code)]
#[derive(Debug)]
pub struct GasStatus {
    pub gas_model_version: u64,
    cost_table: CostTable,
    gas_left: InternalGas,
    gas_price: u64,
    initial_budget: InternalGas,
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

    #[cfg(feature = "gas-profiler")]
    profiler: Option<GasProfiler>,
}

impl GasStatus {
    /// Initialize the gas state with metering enabled.
    ///
    /// Charge for every operation and fail when there is no more gas to pay for operations.
    /// This is the instantiation that must be used when executing a user script.

    pub fn new(cost_table: CostTable, budget: u64, gas_price: u64, gas_model_version: u64) -> Self {
        assert!(gas_price > 0, "gas price cannot be 0");
        let budget_in_unit = budget / gas_price;
        let gas_left = Self::to_internal_units(budget_in_unit);
        let (stack_height_current_tier_mult, stack_height_next_tier_start) =
            cost_table.stack_height_tier(0);
        let (stack_size_current_tier_mult, stack_size_next_tier_start) =
            cost_table.stack_size_tier(0);
        let (instructions_current_tier_mult, instructions_next_tier_start) =
            cost_table.instruction_tier(0);
        Self {
            gas_model_version,
            gas_left,
            gas_price,
            initial_budget: gas_left,
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
            #[cfg(feature = "gas-profiler")]
            profiler: None,
        }
    }

    /// Initialize the gas state with metering disabled.
    ///
    /// It should be used by clients in very specific cases and when executing system
    /// code that does not have to charge the user.
    pub fn new_unmetered() -> Self {
        Self {
            gas_model_version: 4,
            gas_left: InternalGas::new(0),
            gas_price: 1,
            initial_budget: InternalGas::new(0),
            cost_table: ZERO_COST_SCHEDULE.clone(),
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
            #[cfg(feature = "gas-profiler")]
            profiler: None,
        }
    }

    const INTERNAL_UNIT_MULTIPLIER: u64 = 1000;

    fn to_internal_units(val: u64) -> InternalGas {
        InternalGas::new(val * Self::INTERNAL_UNIT_MULTIPLIER)
    }

    #[allow(dead_code)]
    fn to_mist(&self, val: InternalGas) -> u64 {
        let gas: Gas = InternalGas::to_unit_round_down(val);
        u64::from(gas) * self.gas_price
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
        &self.cost_table
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

    // Deduct the amount provided with no conversion, as if it was InternalGasUnit
    fn deduct_units(&mut self, amount: u64) -> PartialVMResult<()> {
        self.deduct_gas(InternalGas::new(amount))
    }

    pub fn set_metering(&mut self, enabled: bool) {
        self.charge = enabled
    }

    // The amount of gas used, it does not include the multiplication for the gas price
    pub fn gas_used_pre_gas_price(&self) -> u64 {
        let gas: Gas = match self.initial_budget.checked_sub(self.gas_left) {
            Some(val) => InternalGas::to_unit_round_down(val),
            None => InternalGas::to_unit_round_down(self.initial_budget),
        };
        u64::from(gas)
    }

    // Charge the number of bytes with the cost per byte value
    // As more bytes are read throughout the computation the cost per bytes is increased.
    pub fn charge_bytes(&mut self, size: usize, cost_per_byte: u64) -> PartialVMResult<()> {
        let computation_cost = if charge_input_as_memory(self.gas_model_version) {
            self.increase_stack_size(size as u64)?;
            self.stack_size_current_tier_mult * size as u64 * cost_per_byte
        } else {
            size as u64 * cost_per_byte
        };
        self.deduct_units(computation_cost)
    }

    fn abstract_memory_size(&self, val: impl ValueView) -> AbstractMemorySize {
        if use_legacy_abstract_size(self.gas_model_version) {
            val.legacy_abstract_memory_size()
        } else {
            val.abstract_memory_size()
        }
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

impl GasMeter for GasStatus {
    /// Charge an instruction and fail if not enough gas units are left.
    fn charge_simple_instr(&mut self, instr: SimpleInstruction) -> PartialVMResult<()> {
        let (pops, pushes, pop_size, push_size) = get_simple_instruction_stack_change(instr);
        self.charge(1, pushes, pops, push_size.into(), pop_size.into())
    }

    fn charge_pop(&mut self, popped_val: impl ValueView) -> PartialVMResult<()> {
        self.charge(1, 0, 1, 0, self.abstract_memory_size(popped_val).into())
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
                    acc + self.abstract_memory_size(elem)
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
            acc + self.abstract_memory_size(elem)
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
            acc + self.abstract_memory_size(elem)
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
            acc + self.abstract_memory_size(elem)
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
        self.charge(1, 1, 0, self.abstract_memory_size(val).into(), 0)
    }

    fn charge_move_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        // Charge for the move of the local on to the stack. Note that we charge here since we
        // aren't tracking the local size (at least not yet). If we were, this should be a net-zero
        // operation in terms of memory usage.
        self.charge(1, 1, 0, self.abstract_memory_size(val).into(), 0)
    }

    fn charge_store_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        // Charge for the storing of the value on the stack into a local. Note here that if we were
        // also accounting for the size of the locals that this would be a net-zero operation in
        // terms of memory.
        self.charge(1, 0, 1, 0, self.abstract_memory_size(val).into())
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
            self.abstract_memory_size(ref_val).into(),
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
            self.abstract_memory_size(new_val).into(),
            self.abstract_memory_size(old_val).into(),
        )
    }

    fn charge_eq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        let size_reduction = self.abstract_memory_size(lhs) + self.abstract_memory_size(rhs);
        self.charge(
            1,
            1,
            2,
            (Type::Bool.size() + size_reduction).into(),
            size_reduction.into(),
        )
    }

    fn charge_neq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        let size_reduction = self.abstract_memory_size(lhs) + self.abstract_memory_size(rhs);
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
            .map(|val| self.abstract_memory_size(val))
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
        // The amount of data on the stack stays constant except we have some extra metadata for
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
        if !self.charge {
            return InternalGas::new(u64::MAX);
        }
        self.gas_left
    }

    #[cfg(feature = "gas-profiler")]
    fn get_profiler_mut(&mut self) -> Option<&mut GasProfiler> {
        self.profiler.as_mut()
    }

    #[cfg(feature = "gas-profiler")]
    fn set_profiler(&mut self, profiler: GasProfiler) {
        self.profiler = Some(profiler);
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

pub fn initial_cost_schedule_v1() -> CostTable {
    let instruction_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (3000, 2),
        (6000, 3),
        (8000, 5),
        (9000, 9),
        (9500, 16),
        (10000, 29),
        (10500, 50),
    ]
    .into_iter()
    .collect();

    let stack_height_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (400, 2),
        (800, 3),
        (1200, 5),
        (1500, 9),
        (1800, 16),
        (2000, 29),
        (2200, 50),
    ]
    .into_iter()
    .collect();

    let stack_size_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (2000, 2),
        (5000, 3),
        (8000, 5),
        (10000, 9),
        (11000, 16),
        (11500, 29),
        (11500, 50),
    ]
    .into_iter()
    .collect();

    CostTable {
        instruction_tiers,
        stack_size_tiers,
        stack_height_tiers,
    }
}

pub fn initial_cost_schedule_v2() -> CostTable {
    let instruction_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (3000, 2),
        (6000, 3),
        (8000, 5),
        (9000, 9),
        (9500, 16),
        (10000, 29),
        (10500, 50),
        (12000, 150),
        (15000, 250),
    ]
    .into_iter()
    .collect();

    let stack_height_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (400, 2),
        (800, 3),
        (1200, 5),
        (1500, 9),
        (1800, 16),
        (2000, 29),
        (2200, 50),
        (3000, 150),
        (5000, 250),
    ]
    .into_iter()
    .collect();

    let stack_size_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (2000, 2),
        (5000, 3),
        (8000, 5),
        (10000, 9),
        (11000, 16),
        (11500, 29),
        (11500, 50),
        (15000, 150),
        (20000, 250),
    ]
    .into_iter()
    .collect();

    CostTable {
        instruction_tiers,
        stack_size_tiers,
        stack_height_tiers,
    }
}

pub fn initial_cost_schedule_v3() -> CostTable {
    let instruction_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (3000, 2),
        (6000, 3),
        (8000, 5),
        (9000, 9),
        (9500, 16),
        (10000, 29),
        (10500, 50),
        (15000, 100),
    ]
    .into_iter()
    .collect();

    let stack_height_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (400, 2),
        (800, 3),
        (1200, 5),
        (1500, 9),
        (1800, 16),
        (2000, 29),
        (2200, 50),
        (5000, 100),
    ]
    .into_iter()
    .collect();

    let stack_size_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (2000, 2),
        (5000, 3),
        (8000, 5),
        (10000, 9),
        (11000, 16),
        (11500, 29),
        (11500, 50),
        (20000, 100),
    ]
    .into_iter()
    .collect();

    CostTable {
        instruction_tiers,
        stack_size_tiers,
        stack_height_tiers,
    }
}

pub fn initial_cost_schedule_v4() -> CostTable {
    let instruction_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (20_000, 2),
        (50_000, 10),
        (100_000, 50),
        (200_000, 100),
    ]
    .into_iter()
    .collect();

    let stack_height_tiers: BTreeMap<u64, u64> =
        vec![(0, 1), (1_000, 2), (10_000, 10)].into_iter().collect();

    let stack_size_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (100_000, 2),     // ~100K
        (500_000, 5),     // ~500K
        (1_000_000, 100), // ~1M
    ]
    .into_iter()
    .collect();

    CostTable {
        instruction_tiers,
        stack_size_tiers,
        stack_height_tiers,
    }
}

pub fn initial_cost_schedule_v5() -> CostTable {
    let instruction_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (20_000, 2),
        (50_000, 10),
        (100_000, 50),
        (200_000, 100),
        (10_000_000, 1000),
    ]
    .into_iter()
    .collect();

    let stack_height_tiers: BTreeMap<u64, u64> =
        vec![(0, 1), (1_000, 2), (10_000, 10)].into_iter().collect();

    let stack_size_tiers: BTreeMap<u64, u64> = vec![
        (0, 1),
        (100_000, 2),        // ~100K
        (500_000, 5),        // ~500K
        (1_000_000, 100),    // ~1M
        (100_000_000, 1000), // ~100M
    ]
    .into_iter()
    .collect();

    CostTable {
        instruction_tiers,
        stack_size_tiers,
        stack_height_tiers,
    }
}

// Convert from our representation of gas costs to the type that the MoveVM expects for unit tests.
// We don't want our gas depending on the MoveVM test utils and we don't want to fix our
// representation to whatever is there, so instead we perform this translation from our gas units
// and cost schedule to the one expected by the Move unit tests.
pub fn initial_cost_schedule_for_unit_tests() -> move_vm_test_utils::gas_schedule::CostTable {
    let table = initial_cost_schedule_v5();
    move_vm_test_utils::gas_schedule::CostTable {
        instruction_tiers: table
            .instruction_tiers
            .into_iter()
            .map(|(k, v)| (k, v))
            .collect(),
        stack_height_tiers: table
            .stack_height_tiers
            .into_iter()
            .map(|(k, v)| (k, v))
            .collect(),
        stack_size_tiers: table
            .stack_size_tiers
            .into_iter()
            .map(|(k, v)| (k, v))
            .collect(),
    }
}
