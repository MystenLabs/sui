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

/// Get the static stack change for a simple instruction.
/// Returns (pops, pushes, pop_size, push_size) for stack accounting.
#[inline]
const fn get_simple_instruction_cost(instr: SimpleInstruction) -> (u64, u64, u64, u64) {
    use SimpleInstruction::*;
    match instr {
        Nop | Ret => (0, 0, 0, 0),
        BrTrue | BrFalse => (1, 0, 1, 0),
        Branch => (0, 0, 0, 0),
        LdU8 => (0, 1, 0, 1),
        LdU16 => (0, 1, 0, 2),
        LdU32 => (0, 1, 0, 4),
        LdU64 => (0, 1, 0, 8),
        LdU128 => (0, 1, 0, 16),
        LdU256 => (0, 1, 0, 32),
        LdTrue | LdFalse => (0, 1, 0, 1),
        FreezeRef => (1, 1, 8, 8),
        MutBorrowLoc | ImmBorrowLoc => (0, 1, 0, 8),
        ImmBorrowField | MutBorrowField | ImmBorrowFieldGeneric | MutBorrowFieldGeneric => {
            (1, 1, 8, 8)
        }
        CastU8 => (1, 1, 1, 1),
        CastU16 => (1, 1, 1, 2),
        CastU32 => (1, 1, 1, 4),
        CastU64 => (1, 1, 1, 8),
        CastU128 => (1, 1, 1, 16),
        CastU256 => (1, 1, 1, 32),
        // Conservative over-approximation for arithmetic: pop smallest, push largest
        Add | Sub | Mul | Mod | Div | BitOr | BitAnd | Xor | Shl | Shr => (2, 1, 2, 32),
        Or | And => (2, 1, 2, 1),
        Not => (1, 1, 1, 1),
        Lt | Gt | Le | Ge => (2, 1, 2, 1),
        Abort => (1, 0, 8, 0),
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
        let (pops, pushes, pop_size, push_size) = get_simple_instruction_cost(instr);
        self.instructions += 1;
        self.pushes += pushes;
        self.pops += pops;
        self.size_increase += push_size;
        self.size_decrease += pop_size;
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
    use crate::shared::gas::UnmeteredGasMeter;

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
}
