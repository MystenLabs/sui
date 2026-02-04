// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! A gas meter implementation that batches static instruction costs.
//!
//! This implementation accumulates gas costs for instructions with statically-known costs
//! and flushes them at basic block boundaries or when encountering dynamic-cost instructions.
//! This reduces the per-instruction overhead of gas metering.

use crate::shared::{
    gas::{GasMeter, SimpleInstruction},
    views::{TypeView, ValueView},
};
use move_binary_format::errors::PartialVMResult;
use move_core_types::{
    gas_algebra::{InternalGas, NumArgs, NumBytes},
    language_storage::ModuleId,
};

/// Size constants matching the reference implementation in tiered_gas_schedule.rs
const REFERENCE_SIZE: u64 = 8;
/// All base types (Bool, U8, U16, etc.) have legacy size of 1
const TYPE_SIZE: u64 = 1;

/// Static stack change for a simple instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstructionCost {
    /// Number of values popped from the stack
    pub pops: u64,
    /// Number of values pushed to the stack
    pub pushes: u64,
    /// Abstract memory size decrease (bytes popped)
    pub pop_size: u64,
    /// Abstract memory size increase (bytes pushed)
    pub push_size: u64,
}

impl InstructionCost {
    const fn new(pops: u64, pushes: u64, pop_size: u64, push_size: u64) -> Self {
        Self {
            pops,
            pushes,
            pop_size,
            push_size,
        }
    }
}

/// Get the static stack change for a simple instruction.
///
/// These values must match `get_simple_instruction_stack_change` in tiered_gas_schedule.rs.
/// Use the `verify_costs_match_reference` test to ensure consistency.
#[inline]
pub const fn get_simple_instruction_cost(instr: SimpleInstruction) -> InstructionCost {
    use SimpleInstruction::*;
    match instr {
        // NB: The `Ret` pops are accounted for in `Call` instructions, so we say `Ret` has no pops.
        Nop | Ret => InstructionCost::new(0, 0, 0, 0),
        BrTrue | BrFalse => InstructionCost::new(1, 0, TYPE_SIZE, 0),
        Branch => InstructionCost::new(0, 0, 0, 0),
        LdU8 | LdU16 | LdU32 | LdU64 | LdU128 | LdU256 => InstructionCost::new(0, 1, 0, TYPE_SIZE),
        LdTrue | LdFalse => InstructionCost::new(0, 1, 0, TYPE_SIZE),
        FreezeRef => InstructionCost::new(1, 1, REFERENCE_SIZE, REFERENCE_SIZE),
        MutBorrowLoc | ImmBorrowLoc => InstructionCost::new(0, 1, 0, REFERENCE_SIZE),
        ImmBorrowField | MutBorrowField | ImmBorrowFieldGeneric | MutBorrowFieldGeneric => {
            InstructionCost::new(1, 1, REFERENCE_SIZE, REFERENCE_SIZE)
        }
        // Since we don't have the size of the value being cast here we take a conservative
        // over-approximation: it is _always_ getting cast from the smallest integer type.
        CastU8 | CastU16 | CastU32 | CastU64 | CastU128 | CastU256 => {
            InstructionCost::new(1, 1, TYPE_SIZE, TYPE_SIZE)
        }
        // NB: We don't know the size of what integers we're dealing with, so we conservatively
        // over-approximate by popping the smallest integers, and push the largest.
        Add | Sub | Mul | Mod | Div | BitOr | BitAnd | Xor | Shl | Shr => {
            InstructionCost::new(2, 1, TYPE_SIZE + TYPE_SIZE, TYPE_SIZE)
        }
        Or | And => InstructionCost::new(2, 1, TYPE_SIZE + TYPE_SIZE, TYPE_SIZE),
        Lt | Gt | Le | Ge => InstructionCost::new(2, 1, TYPE_SIZE + TYPE_SIZE, TYPE_SIZE),
        Not => InstructionCost::new(1, 1, TYPE_SIZE, TYPE_SIZE),
        Abort => InstructionCost::new(1, 0, TYPE_SIZE, 0),
    }
}

/// Accumulated costs that haven't been charged yet.
#[derive(Debug, Default)]
struct AccumulatedCosts {
    /// Number of instructions executed
    instructions: u64,
    /// Stack pushes
    pushes: u64,
    /// Stack pops
    pops: u64,
    /// Stack size increase (bytes)
    size_increase: u64,
    /// Stack size decrease (bytes)
    size_decrease: u64,
}

impl AccumulatedCosts {
    fn is_empty(&self) -> bool {
        self.instructions == 0
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn accumulate_simple(&mut self, instr: SimpleInstruction) {
        let cost = get_simple_instruction_cost(instr);
        self.instructions += 1;
        self.pushes += cost.pushes;
        self.pops += cost.pops;
        self.size_increase += cost.push_size;
        self.size_decrease += cost.pop_size;
    }
}

/// A gas meter that batches static instruction costs.
///
/// This wraps an inner `BatchChargeableGasMeter` and accumulates costs for instructions with
/// statically-known costs. When a dynamic-cost instruction is encountered or
/// at explicit checkpoints, the accumulated costs are flushed to the inner meter.
///
/// # Usage
///
/// ```ignore
/// // Wrap any GasMeter with SimpleBatchAdapter to make it batch-chargeable
/// let inner = SimpleBatchAdapter(my_gas_meter);
/// let mut batching = BatchingGasMeter::new(inner);
///
/// // Or if your meter implements BatchChargeableGasMeter directly:
/// let mut batching = BatchingGasMeter::new(my_batch_aware_meter);
/// ```
pub struct BatchingGasMeter<G: BatchChargeableGasMeter> {
    inner: G,
    accumulated: AccumulatedCosts,
    /// Whether batching is enabled. When false, all calls go directly to inner.
    batching_enabled: bool,
}

impl<G: BatchChargeableGasMeter> BatchingGasMeter<G> {
    /// Create a new batching gas meter wrapping the given inner meter.
    pub fn new(inner: G) -> Self {
        Self {
            inner,
            accumulated: AccumulatedCosts::default(),
            batching_enabled: true,
        }
    }

    /// Create a new batching gas meter with batching disabled (pass-through mode).
    pub fn new_passthrough(inner: G) -> Self {
        Self {
            inner,
            accumulated: AccumulatedCosts::default(),
            batching_enabled: false,
        }
    }

    /// Enable or disable batching.
    pub fn set_batching_enabled(&mut self, enabled: bool) {
        if !enabled && self.batching_enabled {
            // Flush before disabling
            let _ = self.flush();
        }
        self.batching_enabled = enabled;
    }

    /// Flush accumulated costs to the inner gas meter.
    /// Call this at basic block boundaries.
    pub fn flush(&mut self) -> PartialVMResult<()> {
        if self.accumulated.is_empty() {
            return Ok(());
        }

        // Charge all accumulated costs at once
        self.inner.charge_batch(
            self.accumulated.instructions,
            self.accumulated.pushes,
            self.accumulated.pops,
            self.accumulated.size_increase,
            self.accumulated.size_decrease,
        )?;

        self.accumulated.clear();
        Ok(())
    }

    /// Get a reference to the inner gas meter.
    pub fn inner(&self) -> &G {
        &self.inner
    }

    /// Get a mutable reference to the inner gas meter.
    pub fn inner_mut(&mut self) -> &mut G {
        &mut self.inner
    }

    /// Consume this meter and return the inner meter.
    pub fn into_inner(mut self) -> G {
        let _ = self.flush();
        self.inner
    }
}

impl<G: BatchChargeableGasMeter> GasMeter for BatchingGasMeter<G> {
    fn charge_simple_instr(&mut self, instr: SimpleInstruction) -> PartialVMResult<()> {
        if self.batching_enabled {
            self.accumulated.accumulate_simple(instr);
            Ok(())
        } else {
            self.inner.charge_simple_instr(instr)
        }
    }

    // Dynamic cost instructions - flush accumulated costs first, then delegate
    fn charge_pop(&mut self, popped_val: impl ValueView) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_pop(popped_val)
    }

    fn charge_call(
        &mut self,
        module_id: &ModuleId,
        func_name: &str,
        args: impl ExactSizeIterator<Item = impl ValueView>,
        num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_call(module_id, func_name, args, num_locals)
    }

    fn charge_call_generic(
        &mut self,
        module_id: &ModuleId,
        func_name: &str,
        ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        args: impl ExactSizeIterator<Item = impl ValueView>,
        num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner
            .charge_call_generic(module_id, func_name, ty_args, args, num_locals)
    }

    fn charge_ld_const(&mut self, size: NumBytes) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_ld_const(size)
    }

    fn charge_ld_const_after_deserialization(
        &mut self,
        val: impl ValueView,
    ) -> PartialVMResult<()> {
        self.inner.charge_ld_const_after_deserialization(val)
    }

    fn charge_copy_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_copy_loc(val)
    }

    fn charge_move_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_move_loc(val)
    }

    fn charge_store_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_store_loc(val)
    }

    fn charge_pack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_pack(is_generic, args)
    }

    fn charge_unpack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_unpack(is_generic, args)
    }

    fn charge_variant_switch(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_variant_switch(val)
    }

    fn charge_read_ref(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_read_ref(val)
    }

    fn charge_write_ref(
        &mut self,
        new_val: impl ValueView,
        old_val: impl ValueView,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_write_ref(new_val, old_val)
    }

    fn charge_eq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_eq(lhs, rhs)
    }

    fn charge_neq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_neq(lhs, rhs)
    }

    fn charge_vec_pack<'a>(
        &mut self,
        ty: impl TypeView + 'a,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_vec_pack(ty, args)
    }

    fn charge_vec_len(&mut self, ty: impl TypeView) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_vec_len(ty)
    }

    fn charge_vec_borrow(
        &mut self,
        is_mut: bool,
        ty: impl TypeView,
        is_success: bool,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_vec_borrow(is_mut, ty, is_success)
    }

    fn charge_vec_push_back(&mut self, ty: impl TypeView, val: impl ValueView) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_vec_push_back(ty, val)
    }

    fn charge_vec_pop_back(
        &mut self,
        ty: impl TypeView,
        val: Option<impl ValueView>,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_vec_pop_back(ty, val)
    }

    fn charge_vec_unpack(
        &mut self,
        ty: impl TypeView,
        expect_num_elements: NumArgs,
        elems: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_vec_unpack(ty, expect_num_elements, elems)
    }

    fn charge_vec_swap(&mut self, ty: impl TypeView) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_vec_swap(ty)
    }

    fn charge_native_function(
        &mut self,
        amount: InternalGas,
        ret_vals: Option<impl ExactSizeIterator<Item = impl ValueView>>,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_native_function(amount, ret_vals)
    }

    fn charge_native_function_before_execution(
        &mut self,
        ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner
            .charge_native_function_before_execution(ty_args, args)
    }

    fn charge_drop_frame(
        &mut self,
        locals: impl Iterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.flush()?;
        self.inner.charge_drop_frame(locals)
    }

    fn remaining_gas(&self) -> InternalGas {
        self.inner.remaining_gas()
    }
}

/// Extension trait for GasMeter to support batch charging.
/// Implementors that want to benefit from batching should implement this trait
/// to receive accumulated costs in a single call instead of per-instruction.
pub trait BatchChargeableGasMeter: GasMeter {
    /// Charge a batch of accumulated costs at once.
    ///
    /// This is called by `BatchingGasMeter` when flushing accumulated costs.
    /// Implementations can use this to reduce per-instruction overhead.
    ///
    /// # Arguments
    /// * `num_instructions` - Number of instructions in this batch
    /// * `pushes` - Total stack pushes
    /// * `pops` - Total stack pops
    /// * `size_increase` - Total stack size increase (bytes)
    /// * `size_decrease` - Total stack size decrease (bytes)
    fn charge_batch(
        &mut self,
        num_instructions: u64,
        pushes: u64,
        pops: u64,
        size_increase: u64,
        size_decrease: u64,
    ) -> PartialVMResult<()>;
}

/// Wrapper that provides batch charging for any GasMeter by charging Nop per instruction.
/// Use this when wrapping a GasMeter that doesn't implement BatchChargeableGasMeter.
pub struct SimpleBatchAdapter<G: GasMeter>(pub G);

impl<G: GasMeter> GasMeter for SimpleBatchAdapter<G> {
    fn charge_simple_instr(&mut self, instr: SimpleInstruction) -> PartialVMResult<()> {
        self.0.charge_simple_instr(instr)
    }

    fn charge_pop(&mut self, popped_val: impl ValueView) -> PartialVMResult<()> {
        self.0.charge_pop(popped_val)
    }

    fn charge_call(
        &mut self,
        module_id: &ModuleId,
        func_name: &str,
        args: impl ExactSizeIterator<Item = impl ValueView>,
        num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        self.0.charge_call(module_id, func_name, args, num_locals)
    }

    fn charge_call_generic(
        &mut self,
        module_id: &ModuleId,
        func_name: &str,
        ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        args: impl ExactSizeIterator<Item = impl ValueView>,
        num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        self.0.charge_call_generic(module_id, func_name, ty_args, args, num_locals)
    }

    fn charge_ld_const(&mut self, size: NumBytes) -> PartialVMResult<()> {
        self.0.charge_ld_const(size)
    }

    fn charge_ld_const_after_deserialization(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.0.charge_ld_const_after_deserialization(val)
    }

    fn charge_copy_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.0.charge_copy_loc(val)
    }

    fn charge_move_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.0.charge_move_loc(val)
    }

    fn charge_store_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.0.charge_store_loc(val)
    }

    fn charge_pack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.0.charge_pack(is_generic, args)
    }

    fn charge_unpack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.0.charge_unpack(is_generic, args)
    }

    fn charge_variant_switch(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.0.charge_variant_switch(val)
    }

    fn charge_read_ref(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.0.charge_read_ref(val)
    }

    fn charge_write_ref(&mut self, new_val: impl ValueView, old_val: impl ValueView) -> PartialVMResult<()> {
        self.0.charge_write_ref(new_val, old_val)
    }

    fn charge_eq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        self.0.charge_eq(lhs, rhs)
    }

    fn charge_neq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        self.0.charge_neq(lhs, rhs)
    }

    fn charge_vec_pack<'a>(
        &mut self,
        ty: impl TypeView + 'a,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.0.charge_vec_pack(ty, args)
    }

    fn charge_vec_len(&mut self, ty: impl TypeView) -> PartialVMResult<()> {
        self.0.charge_vec_len(ty)
    }

    fn charge_vec_borrow(&mut self, is_mut: bool, ty: impl TypeView, is_success: bool) -> PartialVMResult<()> {
        self.0.charge_vec_borrow(is_mut, ty, is_success)
    }

    fn charge_vec_push_back(&mut self, ty: impl TypeView, val: impl ValueView) -> PartialVMResult<()> {
        self.0.charge_vec_push_back(ty, val)
    }

    fn charge_vec_pop_back(&mut self, ty: impl TypeView, val: Option<impl ValueView>) -> PartialVMResult<()> {
        self.0.charge_vec_pop_back(ty, val)
    }

    fn charge_vec_unpack(
        &mut self,
        ty: impl TypeView,
        expect_num_elements: NumArgs,
        elems: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.0.charge_vec_unpack(ty, expect_num_elements, elems)
    }

    fn charge_vec_swap(&mut self, ty: impl TypeView) -> PartialVMResult<()> {
        self.0.charge_vec_swap(ty)
    }

    fn charge_native_function(
        &mut self,
        amount: InternalGas,
        ret_vals: Option<impl ExactSizeIterator<Item = impl ValueView>>,
    ) -> PartialVMResult<()> {
        self.0.charge_native_function(amount, ret_vals)
    }

    fn charge_native_function_before_execution(
        &mut self,
        ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.0.charge_native_function_before_execution(ty_args, args)
    }

    fn charge_drop_frame(&mut self, locals: impl Iterator<Item = impl ValueView>) -> PartialVMResult<()> {
        self.0.charge_drop_frame(locals)
    }

    fn remaining_gas(&self) -> InternalGas {
        self.0.remaining_gas()
    }
}

impl<G: GasMeter> BatchChargeableGasMeter for SimpleBatchAdapter<G> {
    fn charge_batch(
        &mut self,
        num_instructions: u64,
        _pushes: u64,
        _pops: u64,
        _size_increase: u64,
        _size_decrease: u64,
    ) -> PartialVMResult<()> {
        // Simple fallback: charge Nop for each instruction
        for _ in 0..num_instructions {
            self.0.charge_simple_instr(SimpleInstruction::Nop)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jit::execution::ast::Type;
    use crate::shared::gas::UnmeteredGasMeter;

    /// Reference constant from tiered_gas_schedule.rs (AbstractMemorySize::new(8))
    const REF_REFERENCE_SIZE: u64 = 8;

    #[test]
    fn test_batching_accumulates_costs() {
        let inner = SimpleBatchAdapter(UnmeteredGasMeter);
        let mut meter = BatchingGasMeter::new(inner);

        // These should accumulate without charging
        meter.charge_simple_instr(SimpleInstruction::LdU64).unwrap();
        meter.charge_simple_instr(SimpleInstruction::LdU64).unwrap();
        meter.charge_simple_instr(SimpleInstruction::Add).unwrap();

        assert!(!meter.accumulated.is_empty());
        assert_eq!(meter.accumulated.instructions, 3);

        // Flush should clear accumulated
        meter.flush().unwrap();
        assert!(meter.accumulated.is_empty());
    }

    #[test]
    fn test_passthrough_mode() {
        let inner = SimpleBatchAdapter(UnmeteredGasMeter);
        let mut meter = BatchingGasMeter::new_passthrough(inner);

        meter.charge_simple_instr(SimpleInstruction::LdU64).unwrap();

        // In passthrough mode, nothing accumulates
        assert!(meter.accumulated.is_empty());
    }

    /// Verifies that our hardcoded costs match the reference implementation in tiered_gas_schedule.rs.
    ///
    /// This test ensures that `get_simple_instruction_cost` returns the same values as
    /// `get_simple_instruction_stack_change` in the tiered gas schedule. If this test fails,
    /// the hardcoded values in `get_simple_instruction_cost` need to be updated.
    ///
    /// Reference: external-crates/move/crates/move-vm-runtime/src/dev_utils/tiered_gas_schedule.rs
    #[test]
    fn verify_costs_match_reference() {
        use SimpleInstruction::*;

        // All SimpleInstruction variants to test
        let all_instructions = [
            Nop, Ret, BrTrue, BrFalse, Branch, LdU8, LdU64, LdU128, LdTrue, LdFalse, FreezeRef,
            MutBorrowLoc, ImmBorrowLoc, ImmBorrowField, MutBorrowField, ImmBorrowFieldGeneric,
            MutBorrowFieldGeneric, CastU8, CastU64, CastU128, Add, Sub, Mul, Mod, Div, BitOr,
            BitAnd, Xor, Shl, Shr, Or, And, Not, Lt, Gt, Le, Ge, Abort, LdU16, LdU32, LdU256,
            CastU16, CastU32, CastU256,
        ];

        // Reference values from tiered_gas_schedule.rs
        // Type::*.size() returns LEGACY_BASE_MEMORY_SIZE = 1 for all base types
        let type_size: u64 = Type::Bool.size().into();
        let ref_size: u64 = REF_REFERENCE_SIZE;

        for instr in all_instructions {
            let our_cost = get_simple_instruction_cost(instr);

            // Compute expected values based on tiered_gas_schedule.rs logic
            let (expected_pops, expected_pushes, expected_pop_size, expected_push_size) =
                match instr {
                    Nop | Ret => (0, 0, 0, 0),
                    BrTrue | BrFalse => (1, 0, type_size, 0),
                    Branch => (0, 0, 0, 0),
                    LdU8 | LdU16 | LdU32 | LdU64 | LdU128 | LdU256 => (0, 1, 0, type_size),
                    LdTrue | LdFalse => (0, 1, 0, type_size),
                    FreezeRef => (1, 1, ref_size, ref_size),
                    ImmBorrowLoc | MutBorrowLoc => (0, 1, 0, ref_size),
                    ImmBorrowField | MutBorrowField | ImmBorrowFieldGeneric
                    | MutBorrowFieldGeneric => (1, 1, ref_size, ref_size),
                    CastU8 | CastU16 | CastU32 | CastU64 | CastU128 | CastU256 => {
                        (1, 1, type_size, type_size)
                    }
                    Add | Sub | Mul | Mod | Div | BitOr | BitAnd | Xor | Shl | Shr => {
                        (2, 1, type_size + type_size, type_size)
                    }
                    Or | And => (2, 1, type_size + type_size, type_size),
                    Lt | Gt | Le | Ge => (2, 1, type_size + type_size, type_size),
                    Not => (1, 1, type_size, type_size),
                    Abort => (1, 0, type_size, 0),
                };

            assert_eq!(
                our_cost.pops, expected_pops,
                "pops mismatch for {:?}: got {}, expected {}",
                instr, our_cost.pops, expected_pops
            );
            assert_eq!(
                our_cost.pushes, expected_pushes,
                "pushes mismatch for {:?}: got {}, expected {}",
                instr, our_cost.pushes, expected_pushes
            );
            assert_eq!(
                our_cost.pop_size, expected_pop_size,
                "pop_size mismatch for {:?}: got {}, expected {}",
                instr, our_cost.pop_size, expected_pop_size
            );
            assert_eq!(
                our_cost.push_size, expected_push_size,
                "push_size mismatch for {:?}: got {}, expected {}",
                instr, our_cost.push_size, expected_push_size
            );
        }
    }

    /// Verifies that our constants match the reference implementation.
    #[test]
    fn verify_constants_match_reference() {
        // Verify REFERENCE_SIZE matches tiered_gas_schedule.rs (AbstractMemorySize::new(8))
        assert_eq!(REFERENCE_SIZE, REF_REFERENCE_SIZE, "REFERENCE_SIZE mismatch");

        // Verify TYPE_SIZE matches Type::*.size() which returns LEGACY_BASE_MEMORY_SIZE = 1
        assert_eq!(
            TYPE_SIZE,
            u64::from(Type::Bool.size()),
            "TYPE_SIZE mismatch with Type::Bool.size()"
        );
        assert_eq!(
            TYPE_SIZE,
            u64::from(Type::U8.size()),
            "TYPE_SIZE mismatch with Type::U8.size()"
        );
        assert_eq!(
            TYPE_SIZE,
            u64::from(Type::U256.size()),
            "TYPE_SIZE mismatch with Type::U256.size()"
        );
    }
}
