// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::arena::ArenaPointer,
    dbg_println,
    execution::{
        dispatch_tables::VMDispatchTables,
        interpreter::{
            set_err_info,
            state::{MachineState, ResolvableType, CallStack},
        },
        values::{
            IntegerValue, Reference, Struct, StructRef, VMValueCast, Value, Variant, VariantRef,
            Vector, VectorRef,
        },
    },
    jit::execution::ast::{Bytecode, CallType, Function, Type},
    natives::{extensions::NativeContextExtensions, functions::NativeContext},
    shared::gas::{GasMeter, SimpleInstruction},
};
use fail::fail_point;
use move_binary_format::{errors::*, file_format::JumpTableInner};
use move_core_types::{
    gas_algebra::{NumArgs, NumBytes},
    vm_status::StatusCode,
};
use move_vm_config::runtime::{VMConfig, VMRuntimeLimitsConfig};
use move_vm_profiler::{
    profile_close_frame, profile_close_instr, profile_open_frame, profile_open_instr,
};
use smallvec::SmallVec;

use std::{collections::VecDeque, sync::Arc};

use super::state::ModuleDefinitionResolver;

#[derive(PartialEq, Eq)]
enum StepStatus {
    Running,
    Done,
}

struct RunContext<'vm_cache, 'native, 'native_lifetimes> {
    vtables: &'vm_cache VMDispatchTables,
    vm_config: Arc<VMConfig>,
    extensions: &'native mut NativeContextExtensions<'native_lifetimes>,
}

impl RunContext<'_, '_, '_> {
    // TODO: The Run Context should hold this, not go get it from the Loader.
    fn vm_config(&self) -> &VMConfig {
        &self.vm_config
    }
}

/// Main loop for the execution of a function.
///
/// This runs a newly-made Machine until it is complete. It expects the Machine to have a current
/// call frame set up, with no operands on the stack and no existing call stack. It runs in a loop,
/// calling `step`, until the call stack is empty.
pub(super) fn run(
    start_state: MachineState,
    vtables: &VMDispatchTables,
    vm_config: Arc<VMConfig>,
    extensions: &mut NativeContextExtensions,
    gas_meter: &mut impl GasMeter,
) -> VMResult<Vec<Value>> {
    let mut run_context = RunContext {
        extensions,
        vtables,
        vm_config,
    };

    let mut state = start_state;

    dbg_println!(flag: eval_step, "Call Frame:\n{:?}", state.call_stack.current_frame);
    dbg_println!(flag: eval_step, "{}", {
        let mut buf = String::new();
        let _ = state.debug_print_stack_trace(&mut buf, run_context.vtables);
        buf
    });

    // Run until we're done or we produce an error and bail.
    while step(&mut state, &mut run_context, gas_meter)? != StepStatus::Done {
        println!("-------------------------------------");
        println!("Call Frame:\n{:?}", state.call_stack.current_frame);
        println!("{}", {
            let mut buf = String::new();
            let _ = state.debug_print_stack_trace(&mut buf, run_context.vtables);
            buf
        });
        continue;
    }

    // When we are done, grab the operand stack as the return type.
    let MachineState { operand_stack, call_stack } = state;
    let CallStack { mut heap, current_frame, frames } = call_stack;
    heap.free_stack_frame(current_frame.stack_frame).map_err(|e| e.finish(Location::Undefined))?;
    for frame in frames.into_iter().rev() {
        heap.free_stack_frame(frame.stack_frame).map_err(|e| e.finish(Location::Undefined))?;
    }
    Ok(operand_stack.value)
}

fn step(
    state: &mut MachineState,
    run_context: &mut RunContext,
    gas_meter: &mut impl GasMeter,
) -> VMResult<StepStatus> {
    let fun_ref = state.call_stack.current_frame.function();
    let instructions = fun_ref.code();
    let pc = state.call_stack.current_frame.pc as usize;
    assert!(
        pc <= instructions.len(),
        "PC beyond instruction count for {}",
        fun_ref.name
    );
    let instruction = &instructions[pc];

    fail_point!("move_vm::interpreter_loop", |_| {
        Err(state.set_location(
            PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION)
                .with_message("Injected move_vm::interpreter verifier failure".to_owned()),
        ))
    });

    profile_open_instr!(gas_meter, format!("{:?}", instruction));
    dbg_println!(flag: eval_step, "Instruction: {instruction:?}");
    // These are split out because `PartialVMError` and `VMError` are different types. It's unclear
    // why as they hold identical data, but if we could combine them, we could entirely inline
    // `op_step` into this function.
    match instruction {
        Bytecode::Ret => {
            let charge_result = gas_meter.charge_simple_instr(SimpleInstruction::Ret);
            profile_close_instr!(gas_meter, format!("{:?}", instruction));

            partial_error_to_error(state, run_context, charge_result)?;
            let non_ref_vals = state
                .call_stack
                .current_frame
                .stack_frame
                .drop_all_values()
                .map(|(_idx, val)| val);

            // TODO: Check if the error location is set correctly.
            gas_meter
                .charge_drop_frame(non_ref_vals.into_iter())
                .map_err(|e| state.set_location(e))?;

            profile_close_frame!(
                gas_meter,
                arena::to_ref(current_frame.function).pretty_string()
            );

            if state.can_pop_call_frame() {
                state.pop_call_frame()?;
                // Note: the caller will find the callee's return values at the top of the shared operand stack
                state.call_stack.current_frame.pc += 1; // advance past the Call instruction in the caller
                Ok(StepStatus::Running)
            } else {
                // end of execution. `state` should no longer be used afterward
                Ok(StepStatus::Done)
            }
        }
        Bytecode::CallGeneric(idx) => {
            profile_close_instr!(gas_meter, format!("{:?}", instruction));
            let ty_args = state
                .call_stack
                .current_frame
                .resolver
                .instantiate_generic_function(*idx, state.call_stack.current_frame.ty_args())
                .map_err(|e| set_err_info!(state.call_stack.current_frame, e))?;
            let call_type = state
                .call_stack
                .current_frame
                .resolver
                .function_from_instantiation(*idx);
            let function = call_type_to_function(run_context, call_type)
                .map_err(|err| set_err_info!(state.call_stack.current_frame, err))?;
            call_function(state, run_context, gas_meter, function, ty_args)?;
            Ok(StepStatus::Running)
        }
        Bytecode::VirtualCall(vtable_key) => {
            profile_close_instr!(gas_meter, format!("{:?}", instruction));
            let function = run_context
                .vtables
                .resolve_function(vtable_key)
                .map_err(|err| set_err_info!(state.call_stack.current_frame, err))?;
            call_function(state, run_context, gas_meter, function, vec![])?;
            Ok(StepStatus::Running)
        }
        Bytecode::DirectCall(function) => {
            profile_close_instr!(gas_meter, format!("{:?}", instruction));
            call_function(state, run_context, gas_meter, *function, vec![])?;
            Ok(StepStatus::Running)
        }
        _ => {
            let step_result = op_step_impl(state, run_context, gas_meter, instruction);
            partial_error_to_error(state, run_context, step_result)?;
            Ok(StepStatus::Running)
        }
    }
}

#[inline]
fn control_flow_instruction(instruction: &Bytecode) -> bool {
    match instruction {
        Bytecode::Ret
        | Bytecode::BrTrue(_)
        | Bytecode::BrFalse(_)
        | Bytecode::Branch(_)
        | Bytecode::VariantSwitch(_) => true,

        Bytecode::Pop
        | Bytecode::LdU8(_)
        | Bytecode::LdU64(_)
        | Bytecode::LdU128(_)
        | Bytecode::CastU8
        | Bytecode::CastU64
        | Bytecode::CastU128
        | Bytecode::LdConst(_)
        | Bytecode::LdTrue
        | Bytecode::LdFalse
        | Bytecode::CopyLoc(_)
        | Bytecode::MoveLoc(_)
        | Bytecode::StLoc(_)
        | Bytecode::DirectCall(_)
        | Bytecode::VirtualCall(_)
        | Bytecode::CallGeneric(_)
        | Bytecode::Pack(_)
        | Bytecode::PackGeneric(_)
        | Bytecode::Unpack(_)
        | Bytecode::UnpackGeneric(_)
        | Bytecode::ReadRef
        | Bytecode::WriteRef
        | Bytecode::FreezeRef
        | Bytecode::MutBorrowLoc(_)
        | Bytecode::ImmBorrowLoc(_)
        | Bytecode::MutBorrowField(_)
        | Bytecode::MutBorrowFieldGeneric(_)
        | Bytecode::ImmBorrowField(_)
        | Bytecode::ImmBorrowFieldGeneric(_)
        | Bytecode::Add
        | Bytecode::Sub
        | Bytecode::Mul
        | Bytecode::Mod
        | Bytecode::Div
        | Bytecode::BitOr
        | Bytecode::BitAnd
        | Bytecode::Xor
        | Bytecode::Or
        | Bytecode::And
        | Bytecode::Not
        | Bytecode::Eq
        | Bytecode::Neq
        | Bytecode::Lt
        | Bytecode::Gt
        | Bytecode::Le
        | Bytecode::Ge
        | Bytecode::Abort
        | Bytecode::Nop
        | Bytecode::Shl
        | Bytecode::Shr
        | Bytecode::VecPack(_, _)
        | Bytecode::VecLen(_)
        | Bytecode::VecImmBorrow(_)
        | Bytecode::VecMutBorrow(_)
        | Bytecode::VecPushBack(_)
        | Bytecode::VecPopBack(_)
        | Bytecode::VecUnpack(_, _)
        | Bytecode::VecSwap(_)
        | Bytecode::LdU16(_)
        | Bytecode::LdU32(_)
        | Bytecode::LdU256(_)
        | Bytecode::CastU16
        | Bytecode::CastU32
        | Bytecode::CastU256
        | Bytecode::PackVariant(_)
        | Bytecode::PackVariantGeneric(_)
        | Bytecode::UnpackVariant(_)
        | Bytecode::UnpackVariantImmRef(_)
        | Bytecode::UnpackVariantMutRef(_)
        | Bytecode::UnpackVariantGeneric(_)
        | Bytecode::UnpackVariantGenericImmRef(_)
        | Bytecode::UnpackVariantGenericMutRef(_) => false,
    }
}

#[inline]
fn op_step_impl(
    state: &mut MachineState,
    run_context: &mut RunContext,
    gas_meter: &mut impl GasMeter,
    instruction: &Bytecode,
) -> PartialVMResult<()> {
    use SimpleInstruction as S;

    macro_rules! make_ty {
        ($ty: expr) => {
            ResolvableType {
                ty: $ty,
                vtables: run_context.vtables,
            }
        };
    }

    match instruction {
        // -- CALL/RETURN OPERATIONS -------------
        // These should have been handled in `step` above.
        Bytecode::Ret
        | Bytecode::CallGeneric(_)
        | Bytecode::DirectCall(_)
        | Bytecode::VirtualCall(_) => unreachable!(),
        // -- INTERNAL CONTROL FLOW --------------
        // These all update the current frame's program counter.
        Bytecode::BrTrue(offset) => {
            gas_meter.charge_simple_instr(S::BrTrue)?;
            state.call_stack.current_frame.pc = if state.pop_operand_as::<bool>()? {
                *offset
            } else {
                state.call_stack.current_frame.pc + 1
            };
        }
        Bytecode::BrFalse(offset) => {
            gas_meter.charge_simple_instr(S::BrFalse)?;
            state.call_stack.current_frame.pc = if !state.pop_operand_as::<bool>()? {
                *offset
            } else {
                state.call_stack.current_frame.pc + 1
            };
        }
        Bytecode::Branch(offset) => {
            gas_meter.charge_simple_instr(S::Branch)?;
            state.call_stack.current_frame.pc = *offset;
        }
        Bytecode::VariantSwitch(jump_table_index) => {
            let reference = state.pop_operand_as::<VariantRef>()?;
            gas_meter.charge_variant_switch(&reference)?;
            let tag = reference.get_tag()?;
            let JumpTableInner::Full(jump_table) =
                &state.call_stack.current_frame.function().jump_tables()
                    [jump_table_index.0 as usize]
                    .jump_table;
            state.call_stack.current_frame.pc = jump_table[tag as usize];
        }
        // -- OTHER OPCODES ----------------------
        Bytecode::Pop => {
            let popped_val = state.pop_operand()?;
            gas_meter.charge_pop(popped_val)?;
        }
        Bytecode::LdU8(int_const) => {
            gas_meter.charge_simple_instr(S::LdU8)?;
            state.push_operand(Value::u8(*int_const))?;
        }
        Bytecode::LdU16(int_const) => {
            gas_meter.charge_simple_instr(S::LdU16)?;
            state.push_operand(Value::u16(*int_const))?;
        }
        Bytecode::LdU32(int_const) => {
            gas_meter.charge_simple_instr(S::LdU32)?;
            state.push_operand(Value::u32(*int_const))?;
        }
        Bytecode::LdU64(int_const) => {
            gas_meter.charge_simple_instr(S::LdU64)?;
            state.push_operand(Value::u64(*int_const))?;
        }
        Bytecode::LdU128(int_const) => {
            gas_meter.charge_simple_instr(S::LdU128)?;
            state.push_operand(Value::u128(**int_const))?;
        }
        Bytecode::LdU256(int_const) => {
            gas_meter.charge_simple_instr(S::LdU256)?;
            state.push_operand(Value::u256(**int_const))?;
        }
        Bytecode::LdConst(idx) => {
            let constant = state.call_stack.current_frame.resolver.constant_at(*idx);
            gas_meter.charge_ld_const(NumBytes::new(constant.size))?;
            let val = Value::from_constant_value(constant.value.clone());
            gas_meter.charge_ld_const_after_deserialization(&val)?;
            state.push_operand(val)?
        }
        Bytecode::LdTrue => {
            gas_meter.charge_simple_instr(S::LdTrue)?;
            state.push_operand(Value::bool(true))?;
        }
        Bytecode::LdFalse => {
            gas_meter.charge_simple_instr(S::LdFalse)?;
            state.push_operand(Value::bool(false))?;
        }
        Bytecode::CopyLoc(idx) => {
            // TODO(Gas): We should charge gas before copying the value.
            let local = state
                .call_stack
                .current_frame
                .stack_frame
                .copy_loc(*idx as usize)?;
            gas_meter.charge_copy_loc(&local)?;
            state.push_operand(local)?;
        }
        Bytecode::MoveLoc(idx) => {
            let local = state
                .call_stack
                .current_frame
                .stack_frame
                .move_loc(*idx as usize)?;
            gas_meter.charge_move_loc(&local)?;

            state.push_operand(local)?;
        }
        Bytecode::StLoc(idx) => {
            let value_to_store = state.pop_operand()?;
            gas_meter.charge_store_loc(&value_to_store)?;
            state
                .call_stack
                .current_frame
                .stack_frame
                .store_loc(*idx as usize, value_to_store)?;
        }
        Bytecode::MutBorrowLoc(idx) | Bytecode::ImmBorrowLoc(idx) => {
            let instr = match instruction {
                Bytecode::MutBorrowLoc(_) => S::MutBorrowLoc,
                _ => S::ImmBorrowLoc,
            };
            gas_meter.charge_simple_instr(instr)?;
            state.push_operand(
                state
                    .call_stack
                    .current_frame
                    .stack_frame
                    .borrow_loc(*idx as usize)?,
            )?;
        }
        Bytecode::ImmBorrowField(fh_idx) | Bytecode::MutBorrowField(fh_idx) => {
            let instr = match instruction {
                Bytecode::MutBorrowField(_) => S::MutBorrowField,
                _ => S::ImmBorrowField,
            };
            gas_meter.charge_simple_instr(instr)?;

            let reference = state.pop_operand_as::<StructRef>()?;

            let offset = state
                .call_stack
                .current_frame
                .resolver
                .field_offset(*fh_idx);
            let field_ref = reference.borrow_field(offset)?;
            state.push_operand(field_ref)?;
        }
        Bytecode::ImmBorrowFieldGeneric(fi_idx) | Bytecode::MutBorrowFieldGeneric(fi_idx) => {
            let instr = match instruction {
                Bytecode::MutBorrowField(_) => S::MutBorrowFieldGeneric,
                _ => S::ImmBorrowFieldGeneric,
            };
            gas_meter.charge_simple_instr(instr)?;

            let reference = state.pop_operand_as::<StructRef>()?;

            let offset = state
                .call_stack
                .current_frame
                .resolver
                .field_instantiation_offset(*fi_idx);
            let field_ref = reference.borrow_field(offset)?;
            state.push_operand(field_ref)?;
        }
        Bytecode::Pack(sd_idx) => {
            let field_count = state.call_stack.current_frame.resolver.field_count(*sd_idx);
            let struct_type = state
                .call_stack
                .current_frame
                .resolver
                .get_struct_type(*sd_idx);
            check_depth_of_type(run_context, &struct_type)?;
            gas_meter.charge_pack(false, state.last_n_operands(field_count as usize)?)?;
            let args = state.pop_n_operands(field_count)?;
            state.push_operand(Value::struct_(Struct::pack(args)))?;
        }
        Bytecode::PackGeneric(si_idx) => {
            let field_count = state
                .call_stack
                .current_frame
                .resolver
                .field_instantiation_count(*si_idx);
            let ty = state
                .call_stack
                .current_frame
                .resolver
                .instantiate_struct_type(*si_idx, state.call_stack.current_frame.ty_args())?;
            check_depth_of_type(run_context, &ty)?;
            gas_meter.charge_pack(true, state.last_n_operands(field_count as usize)?)?;
            let args = state.pop_n_operands(field_count)?;
            state.push_operand(Value::struct_(Struct::pack(args)))?;
        }
        Bytecode::Unpack(_sd_idx) => {
            let struct_ = state.pop_operand_as::<Struct>()?;

            gas_meter.charge_unpack(false, struct_.field_views())?;

            for value in struct_.unpack()? {
                state.push_operand(value)?;
            }
        }
        Bytecode::UnpackGeneric(_si_idx) => {
            let struct_ = state.pop_operand_as::<Struct>()?;

            gas_meter.charge_unpack(true, struct_.field_views())?;

            // TODO: Whether or not we want this gas metering in the loop is
            // questionable.  However, if we don't have it in the loop we could wind up
            // doing a fair bit of work before charging for it.
            for value in struct_.unpack()? {
                state.push_operand(value)?;
            }
        }
        Bytecode::ReadRef => {
            let reference = state.pop_operand_as::<Reference>()?;
            gas_meter.charge_read_ref(reference.value_view())?;
            let value = reference.read_ref()?;
            state.push_operand(value)?;
        }
        Bytecode::WriteRef => {
            let reference = state.pop_operand_as::<Reference>()?;
            let value = state.pop_operand()?;
            gas_meter.charge_write_ref(&value, reference.value_view())?;
            reference.write_ref(value)?;
        }
        Bytecode::CastU8 => {
            gas_meter.charge_simple_instr(S::CastU8)?;
            let integer_value = state.pop_operand_as::<IntegerValue>()?;
            state.push_operand(Value::u8(integer_value.cast_u8()?))?;
        }
        Bytecode::CastU16 => {
            gas_meter.charge_simple_instr(S::CastU16)?;
            let integer_value = state.pop_operand_as::<IntegerValue>()?;
            state.push_operand(Value::u16(integer_value.cast_u16()?))?;
        }
        Bytecode::CastU32 => {
            gas_meter.charge_simple_instr(S::CastU16)?;
            let integer_value = state.pop_operand_as::<IntegerValue>()?;
            state.push_operand(Value::u32(integer_value.cast_u32()?))?;
        }
        Bytecode::CastU64 => {
            gas_meter.charge_simple_instr(S::CastU64)?;
            let integer_value = state.pop_operand_as::<IntegerValue>()?;
            state.push_operand(Value::u64(integer_value.cast_u64()?))?;
        }
        Bytecode::CastU128 => {
            gas_meter.charge_simple_instr(S::CastU128)?;
            let integer_value = state.pop_operand_as::<IntegerValue>()?;
            state.push_operand(Value::u128(integer_value.cast_u128()?))?;
        }
        Bytecode::CastU256 => {
            gas_meter.charge_simple_instr(S::CastU16)?;
            let integer_value = state.pop_operand_as::<IntegerValue>()?;
            state.push_operand(Value::u256(integer_value.cast_u256()?))?;
        }
        // Arithmetic Operations
        Bytecode::Add => {
            gas_meter.charge_simple_instr(S::Add)?;
            binop_int(state, IntegerValue::add_checked)?
        }
        Bytecode::Sub => {
            gas_meter.charge_simple_instr(S::Sub)?;
            binop_int(state, IntegerValue::sub_checked)?
        }
        Bytecode::Mul => {
            gas_meter.charge_simple_instr(S::Mul)?;
            binop_int(state, IntegerValue::mul_checked)?
        }
        Bytecode::Mod => {
            gas_meter.charge_simple_instr(S::Mod)?;
            binop_int(state, IntegerValue::rem_checked)?
        }
        Bytecode::Div => {
            gas_meter.charge_simple_instr(S::Div)?;
            binop_int(state, IntegerValue::div_checked)?
        }
        Bytecode::BitOr => {
            gas_meter.charge_simple_instr(S::BitOr)?;
            binop_int(state, IntegerValue::bit_or)?
        }
        Bytecode::BitAnd => {
            gas_meter.charge_simple_instr(S::BitAnd)?;
            binop_int(state, IntegerValue::bit_and)?
        }
        Bytecode::Xor => {
            gas_meter.charge_simple_instr(S::Xor)?;
            binop_int(state, IntegerValue::bit_xor)?
        }
        Bytecode::Shl => {
            gas_meter.charge_simple_instr(S::Shl)?;
            let rhs = state.pop_operand_as::<u8>()?;
            let lhs = state.pop_operand_as::<IntegerValue>()?;
            state.push_operand(lhs.shl_checked(rhs)?.into_value())?;
        }
        Bytecode::Shr => {
            gas_meter.charge_simple_instr(S::Shr)?;
            let rhs = state.pop_operand_as::<u8>()?;
            let lhs = state.pop_operand_as::<IntegerValue>()?;
            state.push_operand(lhs.shr_checked(rhs)?.into_value())?;
        }
        Bytecode::Or => {
            gas_meter.charge_simple_instr(S::Or)?;
            binop_bool(state, |l, r| Ok(l || r))?
        }
        Bytecode::And => {
            gas_meter.charge_simple_instr(S::And)?;
            binop_bool(state, |l, r| Ok(l && r))?
        }
        Bytecode::Lt => {
            gas_meter.charge_simple_instr(S::Lt)?;
            binop_bool(state, IntegerValue::lt)?
        }
        Bytecode::Gt => {
            gas_meter.charge_simple_instr(S::Gt)?;
            binop_bool(state, IntegerValue::gt)?
        }
        Bytecode::Le => {
            gas_meter.charge_simple_instr(S::Le)?;
            binop_bool(state, IntegerValue::le)?
        }
        Bytecode::Ge => {
            gas_meter.charge_simple_instr(S::Ge)?;
            binop_bool(state, IntegerValue::ge)?
        }
        Bytecode::Abort => {
            gas_meter.charge_simple_instr(S::Abort)?;
            let error_code = state.pop_operand_as::<u64>()?;
            let error = PartialVMError::new(StatusCode::ABORTED)
                .with_sub_status(error_code)
                .with_message(format!(
                    "{} at offset {}",
                    state.call_stack.current_frame.function().pretty_string(),
                    state.call_stack.current_frame.pc,
                ));
            return Err(error);
        }
        Bytecode::Eq => {
            let lhs = state.pop_operand()?;
            let rhs = state.pop_operand()?;
            gas_meter.charge_eq(&lhs, &rhs)?;
            state.push_operand(Value::bool(lhs.equals(&rhs)?))?;
        }
        Bytecode::Neq => {
            let lhs = state.pop_operand()?;
            let rhs = state.pop_operand()?;
            gas_meter.charge_neq(&lhs, &rhs)?;
            state.push_operand(Value::bool(!lhs.equals(&rhs)?))?;
        }
        Bytecode::FreezeRef => {
            gas_meter.charge_simple_instr(S::FreezeRef)?;
            // FreezeRef should just be a null op as we don't distinguish between mut
            // and immut ref at runtime.
        }
        Bytecode::Not => {
            gas_meter.charge_simple_instr(S::Not)?;
            let value = !state.pop_operand_as::<bool>()?;
            state.push_operand(Value::bool(value))?;
        }
        Bytecode::Nop => {
            gas_meter.charge_simple_instr(S::Nop)?;
        }
        Bytecode::VecPack(si, num) => {
            let ty = state
                .call_stack
                .current_frame
                .resolver
                .instantiate_single_type(*si, state.call_stack.current_frame.ty_args())?;
            check_depth_of_type(run_context, &ty)?;
            gas_meter.charge_vec_pack(make_ty!(&ty), state.last_n_operands(*num as usize)?)?;
            let elements = state.pop_n_operands(*num as u16)?;
            let value = Vector::pack(&ty, elements)?;
            state.push_operand(value)?;
        }
        Bytecode::VecLen(si) => {
            let vec_ref = state.pop_operand_as::<VectorRef>()?;
            let ty = &state
                .call_stack
                .current_frame
                .resolver
                .instantiate_single_type(*si, state.call_stack.current_frame.ty_args())?;
            gas_meter.charge_vec_len(ResolvableType {
                ty,
                vtables: run_context.vtables,
            })?;
            let value = vec_ref.len(ty)?;
            state.push_operand(value)?;
        }
        Bytecode::VecImmBorrow(si) => {
            let idx = state.pop_operand_as::<u64>()? as usize;
            let vec_ref = state.pop_operand_as::<VectorRef>()?;
            let ty = state
                .call_stack
                .current_frame
                .resolver
                .instantiate_single_type(*si, state.call_stack.current_frame.ty_args())?;
            let res = vec_ref.borrow_elem(idx, &ty);
            gas_meter.charge_vec_borrow(false, make_ty!(&ty), res.is_ok())?;
            state.push_operand(res?)?;
        }
        Bytecode::VecMutBorrow(si) => {
            let idx = state.pop_operand_as::<u64>()? as usize;
            let vec_ref = state.pop_operand_as::<VectorRef>()?;
            let ty = &state
                .call_stack
                .current_frame
                .resolver
                .instantiate_single_type(*si, state.call_stack.current_frame.ty_args())?;
            let res = vec_ref.borrow_elem(idx, ty);
            gas_meter.charge_vec_borrow(true, make_ty!(ty), res.is_ok())?;
            state.push_operand(res?)?;
        }
        Bytecode::VecPushBack(si) => {
            let elem = state.pop_operand()?;
            let vec_ref = state.pop_operand_as::<VectorRef>()?;
            let ty = &state
                .call_stack
                .current_frame
                .resolver
                .instantiate_single_type(*si, state.call_stack.current_frame.ty_args())?;
            gas_meter.charge_vec_push_back(make_ty!(ty), &elem)?;
            vec_ref.push_back(
                elem,
                ty,
                run_context.vm_config.runtime_limits_config.vector_len_max,
            )?;
        }
        Bytecode::VecPopBack(si) => {
            let vec_ref = state.pop_operand_as::<VectorRef>()?;
            let ty = &state
                .call_stack
                .current_frame
                .resolver
                .instantiate_single_type(*si, state.call_stack.current_frame.ty_args())?;
            let res = vec_ref.pop(ty);
            gas_meter.charge_vec_pop_back(make_ty!(ty), res.as_ref().ok())?;
            state.push_operand(res?)?;
        }
        Bytecode::VecUnpack(si, num) => {
            let vec_val = state.pop_operand_as::<Vector>()?;
            let ty = &state
                .call_stack
                .current_frame
                .resolver
                .instantiate_single_type(*si, state.call_stack.current_frame.ty_args())?;
            gas_meter.charge_vec_unpack(make_ty!(ty), NumArgs::new(*num), vec_val.elem_views())?;
            let elements = vec_val.unpack(ty, *num)?;
            for value in elements {
                state.push_operand(value)?;
            }
        }
        Bytecode::VecSwap(si) => {
            let idx2 = state.pop_operand_as::<u64>()? as usize;
            let idx1 = state.pop_operand_as::<u64>()? as usize;
            let vec_ref = state.pop_operand_as::<VectorRef>()?;
            let ty = &state
                .call_stack
                .current_frame
                .resolver
                .instantiate_single_type(*si, state.call_stack.current_frame.ty_args())?;
            gas_meter.charge_vec_swap(make_ty!(ty))?;
            vec_ref.swap(idx1, idx2, ty)?;
        }
        Bytecode::PackVariant(vidx) => {
            let (field_count, variant_tag) = state
                .call_stack
                .current_frame
                .resolver
                .variant_field_count_and_tag(*vidx);
            let enum_type = state.call_stack.current_frame.resolver.get_enum_type(*vidx);
            check_depth_of_type(run_context, &enum_type)?;
            gas_meter.charge_pack(false, state.last_n_operands(field_count as usize)?)?;
            let args = state.pop_n_operands(field_count)?;
            state.push_operand(Value::variant(Variant::pack(variant_tag, args)))?;
        }
        Bytecode::PackVariantGeneric(vidx) => {
            let (field_count, variant_tag) = state
                .call_stack
                .current_frame
                .resolver
                .variant_instantiantiation_field_count_and_tag(*vidx);
            let ty = state
                .call_stack
                .current_frame
                .resolver
                .instantiate_enum_type(*vidx, state.call_stack.current_frame.ty_args())?;
            check_depth_of_type(run_context, &ty)?;
            gas_meter.charge_pack(true, state.last_n_operands(field_count as usize)?)?;
            let args = state.pop_n_operands(field_count)?;
            state.push_operand(Value::variant(Variant::pack(variant_tag, args)))?;
        }
        Bytecode::UnpackVariant(vidx) => {
            let variant = state.pop_operand_as::<Variant>()?;
            let (_, variant_tag) = state
                .call_stack
                .current_frame
                .resolver
                .variant_field_count_and_tag(*vidx);
            gas_meter.charge_unpack(false, variant.field_views())?;
            variant.check_tag(variant_tag)?;
            for value in variant.unpack()? {
                state.push_operand(value)?;
            }
        }
        Bytecode::UnpackVariantImmRef(vidx) | Bytecode::UnpackVariantMutRef(vidx) => {
            let reference = state.pop_operand_as::<VariantRef>()?;
            let (_, variant_tag) = state
                .call_stack
                .current_frame
                .resolver
                .variant_field_count_and_tag(*vidx);
            reference.check_tag(variant_tag)?;
            let references = reference.unpack_variant()?;
            gas_meter.charge_unpack(false, references.iter())?;
            for reference in references {
                state.push_operand(reference)?;
            }
        }
        Bytecode::UnpackVariantGeneric(vidx) => {
            let variant = state.pop_operand_as::<Variant>()?;
            gas_meter.charge_unpack(true, variant.field_views())?;
            let (_, variant_tag) = state
                .call_stack
                .current_frame
                .resolver
                .variant_instantiantiation_field_count_and_tag(*vidx);
            variant.check_tag(variant_tag)?;
            for value in variant.unpack()? {
                state.push_operand(value)?;
            }
        }
        Bytecode::UnpackVariantGenericImmRef(vidx) | Bytecode::UnpackVariantGenericMutRef(vidx) => {
            let reference = state.pop_operand_as::<VariantRef>()?;
            let (_, variant_tag) = state
                .call_stack
                .current_frame
                .resolver
                .variant_instantiantiation_field_count_and_tag(*vidx);
            reference.check_tag(variant_tag)?;
            let references = reference.unpack_variant()?;
            gas_meter.charge_unpack(true, references.iter())?;
            for reference in references {
                state.push_operand(reference)?;
            }
        }
    }
    profile_close_instr!(gas_meter, format!("{:?}", instruction));
    if !control_flow_instruction(instruction) {
        state.call_stack.current_frame.pc += 1;
    }
    Ok(())
}

#[inline]
fn call_type_to_function(
    run_context: &RunContext,
    call_type: &CallType,
) -> PartialVMResult<ArenaPointer<Function>> {
    match call_type {
        CallType::Direct(ptr) => Ok(*ptr),
        CallType::Virtual(vtable_key) => run_context.vtables.resolve_function(vtable_key),
    }
}

fn call_function(
    state: &mut MachineState,
    run_context: &mut RunContext,
    gas_meter: &mut impl GasMeter,
    function: ArenaPointer<Function>,
    ty_args: Vec<Type>,
) -> VMResult<()> {
    let fun_ref = function.to_ref();
    profile_open_frame!(gas_meter, func_name.clone());

    // Charge gas
    let module_id = fun_ref.module_id();
    let last_n_operands = state
        .last_n_operands(fun_ref.arg_count())
        .map_err(|e| set_err_info!(state.call_stack.current_frame, e))?;

    if ty_args.is_empty() {
        // Charge for a non-generic call
        gas_meter
            .charge_call(
                module_id,
                fun_ref.name(),
                last_n_operands,
                (fun_ref.local_count() as u64).into(),
            )
            .map_err(|e| set_err_info!(state.call_stack.current_frame, e))?;
    } else {
        // Charge for a generic call
        gas_meter
            .charge_call_generic(
                module_id,
                fun_ref.name(),
                ty_args.iter().map(|ty| ResolvableType {
                    ty,
                    vtables: run_context.vtables,
                }),
                last_n_operands,
                (fun_ref.local_count() as u64).into(),
            )
            .map_err(|e| set_err_info!(state.call_stack.current_frame, e))?;
    }

    if fun_ref.is_native() {
        call_native(state, run_context, gas_meter, fun_ref, ty_args)?;

        state.call_stack.current_frame.pc += 1; // advance past the Call instruction in the caller

        profile_close_frame!(gas_meter, func_name.clone());
    } else {
        // Note: the caller will find the callee's return values at the top of the shared
        // operand stack when the new frame returns.
        push_call_frame(state, run_context, function, ty_args).map_err(|err| state.maybe_core_dump(err))?;
    }
    Ok(())
}

/// Call a native functions.
fn call_native(
    state: &mut MachineState,
    run_context: &mut RunContext,
    gas_meter: &mut impl GasMeter,
    function: &Function,
    ty_args: Vec<Type>,
) -> VMResult<StepStatus> {
    // Note: refactor if native functions push a frame on the stack
    call_native_impl(state, run_context, gas_meter, function, ty_args).map_err(|e| {
        let id = function.module_id();
        let e = if run_context.vm_config.error_execution_state {
            e.with_exec_state(state.get_internal_state())
        } else {
            e
        };
        e.at_code_offset(function.index(), 0)
            .finish(Location::Module(id.clone()))
    })?;
    Ok(StepStatus::Running)
}

fn call_native_impl(
    state: &mut MachineState,
    run_context: &mut RunContext,
    gas_meter: &mut impl GasMeter,
    function: &Function,
    ty_args: Vec<Type>,
) -> PartialVMResult<()> {
    let expected_args = function.arg_count();
    let arg_count: u16 = match expected_args.try_into() {
        Ok(count) => count,
        Err(_) => {
            return Err(PartialVMError::new(StatusCode::ABORTED));
        }
    };
    let args = state
        .pop_n_operands(arg_count)?
        .into_iter()
        .collect::<VecDeque<_>>();
    let RunContext {
        extensions,
        vm_config,
        vtables,
        ..
    } = run_context;
    let return_values = call_native_with_args(
        Some(state),
        vtables,
        gas_meter,
        &vm_config.runtime_limits_config,
        extensions,
        function,
        &ty_args,
        args,
    )?;
    // Put return values on the top of the operand stack, where the caller will find them.
    // This is one of only two times the operand stack is shared across call stack frames; the other is in handling
    // the Return instruction for normal calls
    for value in return_values {
        state.push_operand(value)?;
    }

    Ok(())
}

pub(super) fn call_native_with_args(
    state: Option<&MachineState>,
    vtables: &VMDispatchTables,
    gas_meter: &mut impl GasMeter,
    runtime_limits_config: &VMRuntimeLimitsConfig,
    extensions: &mut NativeContextExtensions,
    function: &Function,
    ty_args: &[Type],
    args: VecDeque<Value>,
) -> PartialVMResult<SmallVec<[Value; 1]>> {
    let return_type_count = function.return_type_count();
    let expected_args = function.arg_count();
    if args.len() != expected_args {
        return Err(PartialVMError::new(StatusCode::EMPTY_VALUE_STACK));
    }
    let mut native_context = NativeContext::new(
        state,
        vtables,
        extensions,
        runtime_limits_config,
        gas_meter.remaining_gas(),
    );
    let native_function = function.get_native()?;

    gas_meter.charge_native_function_before_execution(
        ty_args.iter().map(|ty| ResolvableType { ty, vtables }),
        args.iter(),
    )?;

    let result = native_function(&mut native_context, ty_args.to_vec(), args)?;

    // Note(Gas): The order by which gas is charged / error gets returned MUST NOT be modified
    //            here or otherwise it becomes an incompatible change!!!
    let return_values = match result.result {
        Ok(vals) => {
            gas_meter.charge_native_function(result.cost, Some(vals.iter()))?;
            vals
        }
        Err(code) => {
            gas_meter
                .charge_native_function(result.cost, Option::<std::iter::Empty<&Value>>::None)?;
            return Err(PartialVMError::new(StatusCode::ABORTED).with_sub_status(code));
        }
    };

    // Paranoid check to protect us against incorrect native function implementations. A native function that
    // returns a different number of values than its declared types will trigger this check
    if return_values.len() != return_type_count {
        return Err(
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                "Arity mismatch: return value count does not match return type count".to_string(),
            ),
        );
    }
    Ok(return_values)
}

/// Perform a binary operation to two values at the top of the stack.
fn binop<F, T>(state: &mut MachineState, f: F) -> PartialVMResult<()>
where
    Value: VMValueCast<T>,
    F: FnOnce(T, T) -> PartialVMResult<Value>,
{
    let rhs = state.pop_operand_as::<T>()?;
    let lhs = state.pop_operand_as::<T>()?;
    let result = f(lhs, rhs)?;
    state.push_operand(result)
}

/// Perform a binary operation for integer values.
fn binop_int<F>(state: &mut MachineState, f: F) -> PartialVMResult<()>
where
    F: FnOnce(IntegerValue, IntegerValue) -> PartialVMResult<IntegerValue>,
{
    binop(state, |lhs, rhs| {
        Ok(match f(lhs, rhs)? {
            IntegerValue::U8(x) => Value::u8(x),
            IntegerValue::U16(x) => Value::u16(x),
            IntegerValue::U32(x) => Value::u32(x),
            IntegerValue::U64(x) => Value::u64(x),
            IntegerValue::U128(x) => Value::u128(x),
            IntegerValue::U256(x) => Value::u256(x),
        })
    })
}

/// Perform a binary operation for boolean values.
fn binop_bool<F, T>(state: &mut MachineState, f: F) -> PartialVMResult<()>
where
    Value: VMValueCast<T>,
    F: FnOnce(T, T) -> PartialVMResult<bool>,
{
    binop(state, |lhs, rhs| Ok(Value::bool(f(lhs, rhs)?)))
}

fn push_call_frame(
    state: &mut MachineState,
    run_context: &RunContext,
    function: ArenaPointer<Function>,
    ty_args: Vec<Type>,
) -> VMResult<()> {
    let fun_ref = function.ptr_clone().to_ref();
    let resolver = ModuleDefinitionResolver::new(run_context.vtables, fun_ref.module_id())
        .map_err(|e| set_err_info!(&state.call_stack.current_frame, e))?;
    let args = state
        .pop_n_operands(fun_ref.arg_count() as u16)
        .map_err(|e| set_err_info!(&state.call_stack.current_frame, e))?;
    state.push_call(resolver, function, ty_args, args)
}

fn partial_error_to_error<T>(
    state: &MachineState,
    run_context: &RunContext,
    result: PartialVMResult<T>,
) -> VMResult<T> {
    result.map_err(|err| {
        let err = if run_context.vm_config().error_execution_state {
            err.with_exec_state(state.get_internal_state())
        } else {
            err
        };
        let err = state.set_location(err.at_code_offset(
            state.call_stack.current_frame.function().index(),
            state.call_stack.current_frame.pc,
        ));
        state.maybe_core_dump(err)
    })
}

fn check_depth_of_type(run_context: &RunContext, ty: &Type) -> PartialVMResult<u64> {
    let Some(max_depth) = run_context
        .vm_config
        .runtime_limits_config
        .max_value_nest_depth
    else {
        return Ok(1);
    };
    check_depth_of_type_impl(run_context, ty, 0, max_depth)
}

fn check_depth_of_type_impl(
    run_context: &RunContext,
    ty: &Type,
    current_depth: u64,
    max_depth: u64,
) -> PartialVMResult<u64> {
    macro_rules! check_depth {
        ($additional_depth:expr) => {
            if current_depth.saturating_add($additional_depth) > max_depth {
                return Err(PartialVMError::new(StatusCode::VM_MAX_VALUE_DEPTH_REACHED));
            } else {
                current_depth.saturating_add($additional_depth)
            }
        };
    }

    // Calculate depth of the type itself
    let ty_depth = match ty {
        Type::Bool
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::U128
        | Type::U256
        | Type::Address
        | Type::Signer => check_depth!(1),
        // Even though this is recursive this is OK since the depth of this recursion is
        // bounded by the depth of the type arguments, which we have already checked.
        Type::Reference(ty) | Type::MutableReference(ty) | Type::Vector(ty) => {
            check_depth_of_type_impl(run_context, ty, check_depth!(1), max_depth)?
        }
        Type::Datatype(si) => {
            let depth_formula = run_context.vtables.calculate_depth_of_type(si)?;
            check_depth!(depth_formula.solve(&[])?)
        }
        Type::DatatypeInstantiation(inst) => {
            let (si, ty_args) = &**inst;
            // Calculate depth of all type arguments, and make sure they themselves are not too deep.
            let ty_arg_depths = ty_args
                .iter()
                .map(|ty| {
                    // Ty args should be fully resolved and not need any type arguments
                    check_depth_of_type_impl(run_context, ty, check_depth!(0), max_depth)
                })
                .collect::<PartialVMResult<Vec<_>>>()?;
            let depth_formula = run_context.vtables.calculate_depth_of_type(si)?;
            check_depth!(depth_formula.solve(&ty_arg_depths)?)
        }
        // NB: substitution must be performed before calling this function
        Type::TyParam(_) => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Type parameter should be fully resolved".to_string()),
            )
        }
    };

    Ok(ty_depth)
}
