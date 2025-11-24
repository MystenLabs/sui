// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::{
    gas_algebra::{AbstractMemorySize, InternalGas, NumArgs, NumBytes},
    language_storage::ModuleId,
};
use move_vm_types::{
    gas::{GasMeter, SimpleInstruction},
    loaded_data::runtime_types::Type,
    views::{SizeConfig, TypeView, ValueView},
};
use sui_types::gas_model::{
    gas_predicates::{native_function_threshold_exceeded, use_legacy_abstract_size},
    tables::{GasStatus, REFERENCE_SIZE, STRUCT_SIZE, VEC_SIZE},
};

pub struct SuiGasMeter<'g>(pub &'g mut GasStatus);

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

impl GasMeter for SuiGasMeter<'_> {
    /// Charge an instruction and fail if not enough gas units are left.
    fn charge_simple_instr(&mut self, instr: SimpleInstruction) -> PartialVMResult<()> {
        let (pops, pushes, pop_size, push_size) = get_simple_instruction_stack_change(instr);
        self.0
            .charge(1, pushes, pops, push_size.into(), pop_size.into())
    }

    fn charge_pop(&mut self, popped_val: impl ValueView) -> PartialVMResult<()> {
        self.0
            .charge(1, 0, 1, 0, abstract_memory_size(self.0, popped_val).into())
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
                    acc + abstract_memory_size(self.0, elem)
                })
            })
            .unwrap_or_else(AbstractMemorySize::zero);
        self.0.record_native_call();
        if native_function_threshold_exceeded(self.0.gas_model_version, self.0.num_native_calls) {
            // Charge for the stack operations. We don't count this as an "instruction" since we
            // already accounted for the `Call` instruction in the
            // `charge_native_function_before_execution` call.
            // The amount returned by the native function is viewed as the "virtual" instruction cost
            // for the native function, and will be charged and contribute to the overall cost tier of
            // the transaction accordingly.
            self.0
                .charge(amount.into(), pushes, 0, size_increase.into(), 0)
        } else {
            // Charge for the stack operations. We don't count this as an "instruction" since we
            // already accounted for the `Call` instruction in the
            // `charge_native_function_before_execution` call.
            self.0.charge(0, pushes, 0, size_increase.into(), 0)?;
            // Now charge the gas that the native function told us to charge.
            self.0.deduct_gas(amount)
        }
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
            acc + abstract_memory_size(self.0, elem)
        });
        // Track that this is going to be popping from the operand stack. We also increment the
        // instruction count as we need to account for the `Call` bytecode that initiated this
        // native call.
        self.0.charge(1, 0, pops, 0, stack_reduction_size.into())
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
            acc + abstract_memory_size(self.0, elem)
        });
        self.0.charge(1, 0, pops, 0, stack_reduction_size.into())
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
            acc + abstract_memory_size(self.0, elem)
        });
        // Charge for the pops, no pushes, and account for the stack size decrease. Also track the
        // `CallGeneric` instruction we must have encountered for this.
        self.0.charge(1, 0, pops, 0, stack_reduction_size.into())
    }

    fn charge_ld_const(&mut self, size: NumBytes) -> PartialVMResult<()> {
        // Charge for the load from the locals onto the stack.
        self.0.charge(1, 1, 0, u64::from(size), 0)
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
        self.0
            .charge(1, 1, 0, abstract_memory_size(self.0, val).into(), 0)
    }

    fn charge_move_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        if reweight_move_loc(self.0.gas_model_version) {
            self.0.charge(1, 1, 0, REFERENCE_SIZE.into(), 0)
        } else {
            // Charge for the move of the local on to the stack. Note that we charge here since we
            // aren't tracking the local size (at least not yet). If we were, this should be a net-zero
            // operation in terms of memory usage.
            self.0
                .charge(1, 1, 0, abstract_memory_size(self.0, val).into(), 0)
        }
    }

    fn charge_store_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        // Charge for the storing of the value on the stack into a local. Note here that if we were
        // also accounting for the size of the locals that this would be a net-zero operation in
        // terms of memory.
        self.0
            .charge(1, 0, 1, 0, abstract_memory_size(self.0, val).into())
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
        self.0.charge(1, 1, num_fields, STRUCT_SIZE.into(), 0)
    }

    fn charge_unpack(
        &mut self,
        _is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        // We perform `num_fields` number of pushes.
        let num_fields = args.len() as u64;
        self.0.charge(1, num_fields, 1, 0, STRUCT_SIZE.into())
    }

    fn charge_variant_switch(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        // We perform a single pop of a value from the stack.
        self.0
            .charge(1, 0, 1, 0, abstract_memory_size(self.0, val).into())
    }

    fn charge_read_ref(&mut self, ref_val: impl ValueView) -> PartialVMResult<()> {
        // We read the reference so we are decreasing the size of the stack by the size of the
        // reference, and adding to it the size of the value that has been read from that
        // reference.
        let size = if reweight_read_ref(self.0.gas_model_version) {
            abstract_memory_size_with_traversal(self.0, ref_val)
        } else {
            abstract_memory_size(self.0, ref_val)
        };
        self.0.charge(1, 1, 1, size.into(), REFERENCE_SIZE.into())
    }

    fn charge_write_ref(
        &mut self,
        new_val: impl ValueView,
        old_val: impl ValueView,
    ) -> PartialVMResult<()> {
        // TODO(tzakian): We should account for this elsewhere as the owner of data the
        // reference points to won't be on the stack. For now though, we treat it as adding to the
        // stack size.
        self.0.charge(
            1,
            1,
            2,
            abstract_memory_size(self.0, new_val).into(),
            abstract_memory_size(self.0, old_val).into(),
        )
    }

    fn charge_eq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        let size_reduction = abstract_memory_size_with_traversal(self.0, lhs)
            + abstract_memory_size_with_traversal(self.0, rhs);
        self.0.charge(
            1,
            1,
            2,
            (Type::Bool.size() + size_reduction).into(),
            size_reduction.into(),
        )
    }

    fn charge_neq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        let size_reduction = abstract_memory_size_with_traversal(self.0, lhs)
            + abstract_memory_size_with_traversal(self.0, rhs);
        let size_increase = if enable_traverse_refs(self.0.gas_model_version) {
            Type::Bool.size() + size_reduction
        } else {
            Type::Bool.size()
        };
        self.0
            .charge(1, 1, 2, size_increase.into(), size_reduction.into())
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
        self.0.charge(1, 1, num_args, VEC_SIZE.into(), 0)
    }

    fn charge_vec_len(&mut self, _ty: impl TypeView) -> PartialVMResult<()> {
        self.0
            .charge(1, 1, 1, Type::U64.size().into(), REFERENCE_SIZE.into())
    }

    fn charge_vec_borrow(
        &mut self,
        _is_mut: bool,
        _ty: impl TypeView,
        _is_success: bool,
    ) -> PartialVMResult<()> {
        self.0.charge(
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
        self.0.charge(1, 0, 2, 0, REFERENCE_SIZE.into())
    }

    fn charge_vec_pop_back(
        &mut self,
        _ty: impl TypeView,
        _val: Option<impl ValueView>,
    ) -> PartialVMResult<()> {
        self.0.charge(1, 1, 1, 0, REFERENCE_SIZE.into())
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
        self.0.charge(1, pushes, 1, 0, VEC_SIZE.into())
    }

    fn charge_vec_swap(&mut self, _ty: impl TypeView) -> PartialVMResult<()> {
        let size_decrease = REFERENCE_SIZE + Type::U64.size() + Type::U64.size();
        self.0.charge(1, 1, 1, 0, size_decrease.into())
    }

    fn charge_drop_frame(
        &mut self,
        _locals: impl Iterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn remaining_gas(&self) -> InternalGas {
        if !self.0.charge {
            return InternalGas::new(u64::MAX);
        }
        self.0.gas_left
    }
}

fn abstract_memory_size(status: &GasStatus, val: impl ValueView) -> AbstractMemorySize {
    let config = size_config_for_gas_model_version(status.gas_model_version, false);
    val.abstract_memory_size(&config)
}

fn abstract_memory_size_with_traversal(
    status: &GasStatus,
    val: impl ValueView,
) -> AbstractMemorySize {
    let config = size_config_for_gas_model_version(status.gas_model_version, true);
    val.abstract_memory_size(&config)
}

fn enable_traverse_refs(gas_model_version: u64) -> bool {
    gas_model_version > 9
}

fn reweight_read_ref(gas_model_version: u64) -> bool {
    // Reweighting `ReadRef` is only done in gas model versions 10 and above.
    gas_model_version > 10
}

fn reweight_move_loc(gas_model_version: u64) -> bool {
    // Reweighting `MoveLoc` is only done in gas model versions 10 and above.
    gas_model_version > 10
}

fn size_config_for_gas_model_version(
    gas_model_version: u64,
    should_traverse_refs: bool,
) -> SizeConfig {
    if use_legacy_abstract_size(gas_model_version) {
        SizeConfig {
            traverse_references: false,
            include_vector_size: false,
        }
    } else if should_traverse_refs {
        SizeConfig {
            traverse_references: enable_traverse_refs(gas_model_version),
            include_vector_size: true,
        }
    } else {
        SizeConfig {
            traverse_references: false,
            include_vector_size: true,
        }
    }
}
