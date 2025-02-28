// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::views::{TypeView, ValueView};
use move_binary_format::errors::PartialVMResult;
use move_core_types::{
    gas_algebra::{InternalGas, NumArgs, NumBytes},
    language_storage::ModuleId,
};

use move_vm_profiler::GasProfiler;

/// Enum of instructions that do not need extra information for gas metering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimpleInstruction {
    Nop,
    Ret,

    BrTrue,
    BrFalse,
    Branch,

    LdU8,
    LdU64,
    LdU128,
    LdTrue,
    LdFalse,

    FreezeRef,
    MutBorrowLoc,
    ImmBorrowLoc,
    ImmBorrowField,
    MutBorrowField,
    ImmBorrowFieldGeneric,
    MutBorrowFieldGeneric,

    CastU8,
    CastU64,
    CastU128,

    Add,
    Sub,
    Mul,
    Mod,
    Div,

    BitOr,
    BitAnd,
    Xor,
    Shl,
    Shr,

    Or,
    And,
    Not,

    Lt,
    Gt,
    Le,
    Ge,

    Abort,

    LdU16,
    LdU32,
    LdU256,
    CastU16,
    CastU32,
    CastU256,
}

/// Trait that defines a generic gas meter interface, allowing clients of the Move VM to implement
/// their own metering scheme.
pub trait GasMeter {
    /// Charge an instruction and fail if not enough gas units are left.
    fn charge_simple_instr(&mut self, instr: SimpleInstruction) -> PartialVMResult<()>;

    fn charge_pop(&mut self, popped_val: impl ValueView) -> PartialVMResult<()>;

    fn charge_call(
        &mut self,
        module_id: &ModuleId,
        func_name: &str,
        args: impl ExactSizeIterator<Item = impl ValueView>,
        num_locals: NumArgs,
    ) -> PartialVMResult<()>;

    fn charge_call_generic(
        &mut self,
        module_id: &ModuleId,
        func_name: &str,
        ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        args: impl ExactSizeIterator<Item = impl ValueView>,
        num_locals: NumArgs,
    ) -> PartialVMResult<()>;

    fn charge_ld_const(&mut self, size: NumBytes) -> PartialVMResult<()>;

    fn charge_ld_const_after_deserialization(&mut self, val: impl ValueView)
        -> PartialVMResult<()>;

    fn charge_copy_loc(&mut self, val: impl ValueView) -> PartialVMResult<()>;

    fn charge_move_loc(&mut self, val: impl ValueView) -> PartialVMResult<()>;

    fn charge_store_loc(&mut self, val: impl ValueView) -> PartialVMResult<()>;

    fn charge_pack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()>;

    fn charge_unpack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()>;

    fn charge_variant_switch(&mut self, val: impl ValueView) -> PartialVMResult<()>;

    fn charge_read_ref(&mut self, val: impl ValueView) -> PartialVMResult<()>;

    fn charge_write_ref(
        &mut self,
        new_val: impl ValueView,
        old_val: impl ValueView,
    ) -> PartialVMResult<()>;

    fn charge_eq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()>;

    fn charge_neq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()>;

    fn charge_vec_pack<'a>(
        &mut self,
        ty: impl TypeView + 'a,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()>;

    fn charge_vec_len(&mut self, ty: impl TypeView) -> PartialVMResult<()>;

    fn charge_vec_borrow(
        &mut self,
        is_mut: bool,
        ty: impl TypeView,
        is_success: bool,
    ) -> PartialVMResult<()>;

    fn charge_vec_push_back(
        &mut self,
        ty: impl TypeView,
        val: impl ValueView,
    ) -> PartialVMResult<()>;

    fn charge_vec_pop_back(
        &mut self,
        ty: impl TypeView,
        val: Option<impl ValueView>,
    ) -> PartialVMResult<()>;

    // TODO(Gas): Expose the elements
    fn charge_vec_unpack(
        &mut self,
        ty: impl TypeView,
        expect_num_elements: NumArgs,
        elems: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()>;

    // TODO(Gas): Expose the two elements
    fn charge_vec_swap(&mut self, ty: impl TypeView) -> PartialVMResult<()>;

    fn charge_native_function(
        &mut self,
        amount: InternalGas,
        ret_vals: Option<impl ExactSizeIterator<Item = impl ValueView>>,
    ) -> PartialVMResult<()>;

    fn charge_native_function_before_execution(
        &mut self,
        ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()>;

    fn charge_drop_frame(
        &mut self,
        locals: impl Iterator<Item = impl ValueView>,
    ) -> PartialVMResult<()>;

    /// Returns the gas left
    fn remaining_gas(&self) -> InternalGas;

    fn get_profiler_mut(&mut self) -> Option<&mut GasProfiler>;

    fn set_profiler(&mut self, profiler: GasProfiler);
}

/// A dummy gas meter that does not meter anything.
/// Charge operations will always succeed.
pub struct UnmeteredGasMeter;

impl GasMeter for UnmeteredGasMeter {
    fn charge_simple_instr(&mut self, _instr: SimpleInstruction) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_pop(&mut self, _popped_val: impl ValueView) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_call(
        &mut self,
        _module_id: &ModuleId,
        _func_name: &str,
        _args: impl IntoIterator<Item = impl ValueView>,
        _num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_call_generic(
        &mut self,
        _module_id: &ModuleId,
        _func_name: &str,
        _ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        _args: impl ExactSizeIterator<Item = impl ValueView>,
        _num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_ld_const(&mut self, _size: NumBytes) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_ld_const_after_deserialization(
        &mut self,
        _val: impl ValueView,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_copy_loc(&mut self, _val: impl ValueView) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_move_loc(&mut self, _val: impl ValueView) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_store_loc(&mut self, _val: impl ValueView) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_pack(
        &mut self,
        _is_generic: bool,
        _args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_unpack(
        &mut self,
        _is_generic: bool,
        _args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_variant_switch(&mut self, _val: impl ValueView) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_read_ref(&mut self, _val: impl ValueView) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_write_ref(
        &mut self,
        _new_val: impl ValueView,
        _old_val: impl ValueView,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_eq(&mut self, _lhs: impl ValueView, _rhs: impl ValueView) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_neq(&mut self, _lhs: impl ValueView, _rhs: impl ValueView) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_vec_pack<'a>(
        &mut self,
        _ty: impl TypeView + 'a,
        _args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_vec_len(&mut self, _ty: impl TypeView) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_vec_borrow(
        &mut self,
        _is_mut: bool,
        _ty: impl TypeView,
        _is_success: bool,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_vec_push_back(
        &mut self,
        _ty: impl TypeView,
        _val: impl ValueView,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_vec_pop_back(
        &mut self,
        _ty: impl TypeView,
        _val: Option<impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_vec_unpack(
        &mut self,
        _ty: impl TypeView,
        _expect_num_elements: NumArgs,
        _elems: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_vec_swap(&mut self, _ty: impl TypeView) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_native_function(
        &mut self,
        _amount: InternalGas,
        _ret_vals: Option<impl ExactSizeIterator<Item = impl ValueView>>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_native_function_before_execution(
        &mut self,
        _ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        _args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_drop_frame(
        &mut self,
        _locals: impl Iterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn remaining_gas(&self) -> InternalGas {
        InternalGas::new(u64::MAX)
    }

    fn get_profiler_mut(&mut self) -> Option<&mut GasProfiler> {
        None
    }

    fn set_profiler(&mut self, _profiler: GasProfiler) {}
}
