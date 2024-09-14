// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    interpreter::state::{CallFrame, MachineState, TypeWithLoader},
    loader::{
        arena::ArenaPointer,
        ast::{Bytecode, Function},
        Loader, ModuleDefinitionResolver,
    },
    native_extensions::NativeContextExtensions,
    native_functions::NativeContext,
    trace,
};
use fail::fail_point;
use move_binary_format::{errors::*, file_format::JumpTableInner};
use move_core_types::{
    account_address::AccountAddress,
    gas_algebra::{NumArgs, NumBytes},
    vm_status::StatusCode,
};
use move_vm_config::runtime::{VMConfig, VMRuntimeLimitsConfig};
#[cfg(feature = "gas-profiler")]
use move_vm_profiler::GasProfiler;
use move_vm_profiler::{
    profile_close_frame, profile_close_instr, profile_open_frame, profile_open_instr,
};
use move_vm_types::{
    data_store::DataStore,
    gas::{GasMeter, SimpleInstruction},
    loaded_data::runtime_types::Type,
    values::{
        IntegerValue, Reference, Struct, StructRef, VMValueCast, Value, Variant, VariantRef,
        Vector, VectorRef,
    },
};
use smallvec::SmallVec;

use std::collections::VecDeque;

const DEBUG_STEP_PRINT: bool = false;

#[derive(PartialEq, Eq)]
enum StepStatus {
    Running,
    Done,
}

struct RunContext<'loader, 'native, 'native_lifetimes> {
    loader: &'loader Loader,
    link_context: AccountAddress,
    defn_resolver: ModuleDefinitionResolver<'loader>,
    extensions: &'native mut NativeContextExtensions<'native_lifetimes>,
}

impl RunContext<'_, '_, '_> {
    // TODO: The Run Context should hold this, not go get it from the Loader.
    fn vm_config(&self) -> &VMConfig {
        self.loader.vm_config()
    }
}

macro_rules! set_err_info {
    ($frame:expr, $e:expr) => {{
        let function = $frame.function();
        $e.at_code_offset(function.index(), $frame.pc)
            .finish($frame.location())
    }};
}

/// Main loop for the execution of a function.
///
/// This runs a newly-made Machine until it is complete. It expects the Machine to have a current
/// call frame set up, with no operands on the stack and no existing call stack. It runs in a loop,
/// calling `step`, until the call stack is empty.
pub(super) fn run(
    start_state: MachineState,
    data_store: &impl DataStore,
    gas_meter: &mut impl GasMeter,
    extensions: &mut NativeContextExtensions,
    loader: &Loader,
) -> VMResult<Vec<Value>> {
    let link_context = data_store.link_context();
    let resolver = start_state.current_frame.resolver(link_context, loader);

    let mut run_context = RunContext {
        loader,
        link_context,
        defn_resolver: resolver,
        extensions,
    };

    let mut state = start_state;

    if DEBUG_STEP_PRINT {
        let mut buf = String::new();
        let _ = state.debug_print_stack_trace(&mut buf, loader);
        println!("Call Frame:\n{:?}", state.current_frame);
        println!("{buf}");
    }

    // Run until we're done or we produce an error and bail.
    while step(&mut state, &mut run_context, gas_meter)? != StepStatus::Done {
        if DEBUG_STEP_PRINT {
            let mut buf = String::new();
            println!("-------------------------------------");
            println!("Call Frame:\n{:?}", state.current_frame);
            let _ = state.debug_print_stack_trace(&mut buf, loader);
            println!("{buf}");
        }
        continue;
    }

    // When we are done, grab the operand stack as the return type.
    let MachineState { operand_stack, .. } = state;
    Ok(operand_stack.value)
}

fn step(
    state: &mut MachineState,
    run_context: &mut RunContext,
    gas_meter: &mut impl GasMeter,
) -> VMResult<StepStatus> {
    let fun_ref = state.current_frame.function();
    let instructions = fun_ref.code();
    let pc = state.current_frame.pc as usize;
    assert!(
        pc <= instructions.len(),
        "PC beyond instruction count for {}",
        fun_ref.name
    );
    let instruction = &instructions[pc];

    trace!(
        fun_ref,
        &state.current_frame.locals,
        state.current_frame.pc,
        instruction,
        run_context.defn_resolver,
        state
    );

    fail_point!("move_vm::interpreter_loop", |_| {
        Err(state.set_location(
            PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION)
                .with_message("Injected move_vm::interpreter verifier failure".to_owned()),
        ))
    });

    profile_open_instr!(gas_meter, format!("{:?}", instruction));
    if DEBUG_STEP_PRINT {
        println!("Instruction: {instruction:?}");
    }
    // These are split out because `PartialVMError` and `VMError` are different types. It's unclear
    // why as they hold identical data, but if we could combine them, we could entirely inline
    // `op_step` into this function.
    match instruction {
        Bytecode::Ret => {
            let charge_result = gas_meter.charge_simple_instr(SimpleInstruction::Ret);
            profile_close_instr!(gas_meter, format!("{:?}", instruction));

            partial_error_to_error(state, run_context, charge_result)?;
            let non_ref_vals = state
                .current_frame
                .locals
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

            if let Some(frame) = state.pop_call_frame() {
                // Note: the caller will find the callee's return values at the top of the shared operand stack
                state.current_frame = frame;
                state.current_frame.pc += 1; // advance past the Call instruction in the caller
                replace_resolver(state, run_context);
                Ok(StepStatus::Running)
            } else {
                // end of execution. `state` should no longer be used afterward
                Ok(StepStatus::Done)
            }
        }
        Bytecode::CallGeneric(idx) => {
            profile_close_instr!(gas_meter, format!("{:?}", instruction));

            let ty_args = run_context
                .defn_resolver
                .instantiate_generic_function(*idx, state.current_frame.ty_args())
                .map_err(|e| set_err_info!(state.current_frame, e))?;
            let function = run_context.defn_resolver.function_from_instantiation(*idx);
            let fun_ref = function.to_ref();

            #[cfg(feature = "gas-profiler")]
            let func_name = fun_ref.pretty_string();
            profile_open_frame!(gas_meter, func_name.clone());

            // Charge gas
            let module_id = fun_ref.module_id();
            let last_n_operands = state
                .last_n_operands(fun_ref.arg_count())
                .map_err(|e| set_err_info!(state.current_frame, e))?;
            gas_meter
                .charge_call_generic(
                    module_id,
                    fun_ref.name(),
                    ty_args.iter().map(|ty| TypeWithLoader {
                        ty,
                        loader: run_context.loader,
                    }),
                    last_n_operands,
                    (fun_ref.local_count() as u64).into(),
                )
                .map_err(|e| set_err_info!(state.current_frame, e))?;

            call_function(state, run_context, gas_meter, function, ty_args)?;
            replace_resolver(state, run_context);
            Ok(StepStatus::Running)
        }
        Bytecode::VirtualCall(_vtable_key) => {
            unimplemented!("TODO: implement virtual calls when adding vtables to interpreter")
        }
        Bytecode::StaticCall(function) => {
            profile_close_instr!(gas_meter, format!("{:?}", instruction));

            let fun_ref = function.to_ref();
            #[cfg(feature = "gas-profiler")]
            let func_name = func.pretty_string();
            profile_open_frame!(gas_meter, func_name.clone());

            // Charge gas
            let module_id = fun_ref.module_id();
            let last_n_operands = state
                .last_n_operands(fun_ref.arg_count())
                .map_err(|e| set_err_info!(state.current_frame, e))?;
            gas_meter
                .charge_call(
                    module_id,
                    fun_ref.name(),
                    last_n_operands,
                    (fun_ref.local_count() as u64).into(),
                )
                .map_err(|e| set_err_info!(state.current_frame, e))?;
            call_function(state, run_context, gas_meter, *function, vec![])?;
            replace_resolver(state, run_context);
            Ok(StepStatus::Running)
        }
        _ => {
            let step_result = op_step_impl(state, run_context, gas_meter, instruction);
            partial_error_to_error(state, run_context, step_result)?;
            Ok(StepStatus::Running)
        }
    }
}

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
        | Bytecode::StaticCall(_)
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

fn op_step_impl(
    state: &mut MachineState,
    run_context: &mut RunContext,
    gas_meter: &mut impl GasMeter,
    instruction: &Bytecode,
) -> PartialVMResult<()> {
    use SimpleInstruction as S;

    macro_rules! make_ty {
        ($ty: expr) => {
            TypeWithLoader {
                ty: $ty,
                loader: run_context.loader,
            }
        };
    }

    match instruction {
        // -- CALL/RETURN OPERATIONS -------------
        // These should have been handled in `step` above.
        Bytecode::Ret
        | Bytecode::CallGeneric(_)
        | Bytecode::StaticCall(_)
        | Bytecode::VirtualCall(_) => unreachable!(),
        // -- INTERNAL CONTROL FLOW --------------
        // These all update the current frame's program counter.
        Bytecode::BrTrue(offset) => {
            gas_meter.charge_simple_instr(S::BrTrue)?;
            state.current_frame.pc = if state.pop_operand_as::<bool>()? {
                *offset
            } else {
                state.current_frame.pc + 1
            };
        }
        Bytecode::BrFalse(offset) => {
            gas_meter.charge_simple_instr(S::BrFalse)?;
            state.current_frame.pc = if !state.pop_operand_as::<bool>()? {
                *offset
            } else {
                state.current_frame.pc + 1
            };
        }
        Bytecode::Branch(offset) => {
            gas_meter.charge_simple_instr(S::Branch)?;
            state.current_frame.pc = *offset;
        }
        Bytecode::VariantSwitch(jump_table_index) => {
            let reference = state.pop_operand_as::<VariantRef>()?;
            gas_meter.charge_variant_switch(&reference)?;
            let tag = reference.get_tag()?;
            let JumpTableInner::Full(jump_table) = &state.current_frame.function().jump_tables()
                [jump_table_index.0 as usize]
                .jump_table;
            state.current_frame.pc = jump_table[tag as usize];
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
            let constant = run_context.defn_resolver.constant_at(*idx);
            gas_meter.charge_ld_const(NumBytes::new(constant.data.len() as u64))?;

            let val = Value::deserialize_constant(constant).ok_or_else(|| {
                PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(
                    "Verifier failed to verify the deserialization of constants".to_owned(),
                )
            })?;

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
            let local = state.current_frame.locals.copy_loc(*idx as usize)?;
            gas_meter.charge_copy_loc(&local)?;
            state.push_operand(local)?;
        }
        Bytecode::MoveLoc(idx) => {
            let local = state.current_frame.locals.move_loc(
                *idx as usize,
                run_context
                    .vm_config()
                    .enable_invariant_violation_check_in_swap_loc,
            )?;
            gas_meter.charge_move_loc(&local)?;

            state.push_operand(local)?;
        }
        Bytecode::StLoc(idx) => {
            let value_to_store = state.pop_operand()?;
            gas_meter.charge_store_loc(&value_to_store)?;
            state.current_frame.locals.store_loc(
                *idx as usize,
                value_to_store,
                run_context
                    .vm_config()
                    .enable_invariant_violation_check_in_swap_loc,
            )?;
        }
        Bytecode::MutBorrowLoc(idx) | Bytecode::ImmBorrowLoc(idx) => {
            let instr = match instruction {
                Bytecode::MutBorrowLoc(_) => S::MutBorrowLoc,
                _ => S::ImmBorrowLoc,
            };
            gas_meter.charge_simple_instr(instr)?;
            state.push_operand(state.current_frame.locals.borrow_loc(*idx as usize)?)?;
        }
        Bytecode::ImmBorrowField(fh_idx) | Bytecode::MutBorrowField(fh_idx) => {
            let instr = match instruction {
                Bytecode::MutBorrowField(_) => S::MutBorrowField,
                _ => S::ImmBorrowField,
            };
            gas_meter.charge_simple_instr(instr)?;

            let reference = state.pop_operand_as::<StructRef>()?;

            let offset = run_context.defn_resolver.field_offset(*fh_idx);
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

            let offset = run_context
                .defn_resolver
                .field_instantiation_offset(*fi_idx);
            let field_ref = reference.borrow_field(offset)?;
            state.push_operand(field_ref)?;
        }
        Bytecode::Pack(sd_idx) => {
            let field_count = run_context.defn_resolver.field_count(*sd_idx);
            let struct_type = run_context.defn_resolver.get_struct_type(*sd_idx);
            check_depth_of_type(&run_context.defn_resolver, &struct_type)?;
            gas_meter.charge_pack(false, state.last_n_operands(field_count as usize)?)?;
            let args = state.pop_n_operands(field_count)?;
            state.push_operand(Value::struct_(Struct::pack(args)))?;
        }
        Bytecode::PackGeneric(si_idx) => {
            let field_count = run_context.defn_resolver.field_instantiation_count(*si_idx);
            let ty = run_context
                .defn_resolver
                .instantiate_struct_type(*si_idx, state.current_frame.ty_args())?;
            check_depth_of_type(&run_context.defn_resolver, &ty)?;
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
                    state.current_frame.function().pretty_string(),
                    state.current_frame.pc,
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
            let ty = run_context
                .defn_resolver
                .instantiate_single_type(*si, state.current_frame.ty_args())?;
            check_depth_of_type(&run_context.defn_resolver, &ty)?;
            gas_meter.charge_vec_pack(make_ty!(&ty), state.last_n_operands(*num as usize)?)?;
            let elements = state.pop_n_operands(*num as u16)?;
            let value = Vector::pack(&ty, elements)?;
            state.push_operand(value)?;
        }
        Bytecode::VecLen(si) => {
            let vec_ref = state.pop_operand_as::<VectorRef>()?;
            let ty = &run_context
                .defn_resolver
                .instantiate_single_type(*si, state.current_frame.ty_args())?;
            gas_meter.charge_vec_len(TypeWithLoader {
                ty,
                loader: run_context.loader,
            })?;
            let value = vec_ref.len(ty)?;
            state.push_operand(value)?;
        }
        Bytecode::VecImmBorrow(si) => {
            let idx = state.pop_operand_as::<u64>()? as usize;
            let vec_ref = state.pop_operand_as::<VectorRef>()?;
            let ty = run_context
                .defn_resolver
                .instantiate_single_type(*si, state.current_frame.ty_args())?;
            let res = vec_ref.borrow_elem(idx, &ty);
            gas_meter.charge_vec_borrow(false, make_ty!(&ty), res.is_ok())?;
            state.push_operand(res?)?;
        }
        Bytecode::VecMutBorrow(si) => {
            let idx = state.pop_operand_as::<u64>()? as usize;
            let vec_ref = state.pop_operand_as::<VectorRef>()?;
            let ty = &run_context
                .defn_resolver
                .instantiate_single_type(*si, state.current_frame.ty_args())?;
            let res = vec_ref.borrow_elem(idx, ty);
            gas_meter.charge_vec_borrow(true, make_ty!(ty), res.is_ok())?;
            state.push_operand(res?)?;
        }
        Bytecode::VecPushBack(si) => {
            let elem = state.pop_operand()?;
            let vec_ref = state.pop_operand_as::<VectorRef>()?;
            let ty = &run_context
                .defn_resolver
                .instantiate_single_type(*si, state.current_frame.ty_args())?;
            gas_meter.charge_vec_push_back(make_ty!(ty), &elem)?;
            vec_ref.push_back(elem, ty, state.runtime_limits_config().vector_len_max)?;
        }
        Bytecode::VecPopBack(si) => {
            let vec_ref = state.pop_operand_as::<VectorRef>()?;
            let ty = &run_context
                .defn_resolver
                .instantiate_single_type(*si, state.current_frame.ty_args())?;
            let res = vec_ref.pop(ty);
            gas_meter.charge_vec_pop_back(make_ty!(ty), res.as_ref().ok())?;
            state.push_operand(res?)?;
        }
        Bytecode::VecUnpack(si, num) => {
            let vec_val = state.pop_operand_as::<Vector>()?;
            let ty = &run_context
                .defn_resolver
                .instantiate_single_type(*si, state.current_frame.ty_args())?;
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
            let ty = &run_context
                .defn_resolver
                .instantiate_single_type(*si, state.current_frame.ty_args())?;
            gas_meter.charge_vec_swap(make_ty!(ty))?;
            vec_ref.swap(idx1, idx2, ty)?;
        }
        Bytecode::PackVariant(vidx) => {
            let (field_count, variant_tag) =
                run_context.defn_resolver.variant_field_count_and_tag(*vidx);
            let enum_type = run_context.defn_resolver.get_enum_type(*vidx);
            check_depth_of_type(&run_context.defn_resolver, &enum_type)?;
            gas_meter.charge_pack(false, state.last_n_operands(field_count as usize)?)?;
            let args = state.pop_n_operands(field_count)?;
            state.push_operand(Value::variant(Variant::pack(variant_tag, args)))?;
        }
        Bytecode::PackVariantGeneric(vidx) => {
            let (field_count, variant_tag) = run_context
                .defn_resolver
                .variant_instantiantiation_field_count_and_tag(*vidx);
            let ty = run_context
                .defn_resolver
                .instantiate_enum_type(*vidx, state.current_frame.ty_args())?;
            check_depth_of_type(&run_context.defn_resolver, &ty)?;
            gas_meter.charge_pack(true, state.last_n_operands(field_count as usize)?)?;
            let args = state.pop_n_operands(field_count)?;
            state.push_operand(Value::variant(Variant::pack(variant_tag, args)))?;
        }
        Bytecode::UnpackVariant(vidx) => {
            let variant = state.pop_operand_as::<Variant>()?;
            let (_, variant_tag) = run_context.defn_resolver.variant_field_count_and_tag(*vidx);
            gas_meter.charge_unpack(false, variant.field_views())?;
            variant.check_tag(variant_tag)?;
            for value in variant.unpack()? {
                state.push_operand(value)?;
            }
        }
        Bytecode::UnpackVariantImmRef(vidx) | Bytecode::UnpackVariantMutRef(vidx) => {
            let reference = state.pop_operand_as::<VariantRef>()?;
            let (_, variant_tag) = run_context.defn_resolver.variant_field_count_and_tag(*vidx);
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
            let (_, variant_tag) = run_context
                .defn_resolver
                .variant_instantiantiation_field_count_and_tag(*vidx);
            variant.check_tag(variant_tag)?;
            for value in variant.unpack()? {
                state.push_operand(value)?;
            }
        }
        Bytecode::UnpackVariantGenericImmRef(vidx) | Bytecode::UnpackVariantGenericMutRef(vidx) => {
            let reference = state.pop_operand_as::<VariantRef>()?;
            let (_, variant_tag) = run_context
                .defn_resolver
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
        state.current_frame.pc += 1;
    }
    Ok(())
}

fn call_function(
    state: &mut MachineState,
    run_context: &mut RunContext,
    gas_meter: &mut impl GasMeter,
    function: ArenaPointer<Function>,
    ty_args: Vec<Type>,
) -> VMResult<()> {
    let fun_ref = function.to_ref();
    if fun_ref.is_native() {
        call_native(state, run_context, gas_meter, fun_ref, ty_args)?;

        state.current_frame.pc += 1; // advance past the Call instruction in the caller

        profile_close_frame!(gas_meter, func_name.clone());
    } else {
        let new_frame = make_call_frame(state, function, ty_args)
            .map_err(|e| state.set_location(e))
            .map_err(|err| state.maybe_core_dump(err))?;
        // Note: the caller will find the callee's return values at the top of the shared
        // operand stack when the new frame returns.
        state.push_call_frame(new_frame)?;
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
        let e = if run_context.loader.vm_config().error_execution_state {
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
        defn_resolver: resolver,
        extensions,
        ..
    } = run_context;
    let return_values = call_native_with_args(
        Some(state),
        resolver,
        gas_meter,
        state.runtime_limits_config(),
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
    resolver: &ModuleDefinitionResolver,
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
        resolver,
        extensions,
        runtime_limits_config,
        gas_meter.remaining_gas(),
    );
    let native_function = function.get_native()?;

    gas_meter.charge_native_function_before_execution(
        ty_args.iter().map(|ty| TypeWithLoader {
            ty,
            loader: resolver.loader(),
        }),
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

fn replace_resolver(state: &MachineState, run_context: &mut RunContext) {
    let resolver = state
        .current_frame
        .resolver(run_context.link_context, run_context.loader);
    let _ = std::mem::replace(&mut run_context.defn_resolver, resolver);
}

fn make_call_frame(
    state: &mut MachineState,
    function: ArenaPointer<Function>,
    ty_args: Vec<Type>,
) -> PartialVMResult<CallFrame> {
    let fun_ref = function.to_ref();
    let args = state.pop_n_operands(fun_ref.arg_count() as u16)?;
    Ok(CallFrame::new(function, ty_args, args))
}

fn partial_error_to_error<T>(
    state: &MachineState,
    run_context: &RunContext,
    result: PartialVMResult<T>,
) -> VMResult<T> {
    result.map_err(|err| {
        // TODO: The run context should just hold the VM Config ref, not go get it from the Loader.
        let err = if run_context.vm_config().error_execution_state {
            err.with_exec_state(state.get_internal_state())
        } else {
            err
        };
        let err = state.set_location(err.at_code_offset(
            state.current_frame.function().index(),
            state.current_frame.pc,
        ));
        state.maybe_core_dump(err)
    })
}

fn check_depth_of_type(resolver: &ModuleDefinitionResolver, ty: &Type) -> PartialVMResult<u64> {
    let Some(max_depth) = resolver
        .loader()
        .vm_config()
        .runtime_limits_config
        .max_value_nest_depth
    else {
        return Ok(1);
    };
    check_depth_of_type_impl(resolver, ty, 0, max_depth)
}

fn check_depth_of_type_impl(
    resolver: &ModuleDefinitionResolver,
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
            check_depth_of_type_impl(resolver, ty, check_depth!(1), max_depth)?
        }
        Type::Datatype(si) => {
            let struct_type = resolver.loader().get_type(*si).ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Struct Definition not resolved".to_string())
            })?;
            check_depth!(struct_type
                .depth
                .as_ref()
                .ok_or_else(|| { PartialVMError::new(StatusCode::VM_MAX_VALUE_DEPTH_REACHED) })?
                .solve(&[])?)
        }
        Type::DatatypeInstantiation(inst) => {
            let (si, ty_args) = &**inst;
            // Calculate depth of all type arguments, and make sure they themselves are not too deep.
            let ty_arg_depths = ty_args
                .iter()
                .map(|ty| {
                    // Ty args should be fully resolved and not need any type arguments
                    check_depth_of_type_impl(resolver, ty, check_depth!(0), max_depth)
                })
                .collect::<PartialVMResult<Vec<_>>>()?;
            let struct_type = resolver.loader().get_type(*si).ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Struct Definition not resolved".to_string())
            })?;
            check_depth!(struct_type
                .depth
                .as_ref()
                .ok_or_else(|| { PartialVMError::new(StatusCode::VM_MAX_VALUE_DEPTH_REACHED) })?
                .solve(&ty_arg_depths)?)
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
