// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines the transfer functions for verifying reference safety of a procedure body.
//! The checks include (but are not limited to)
//! - verifying that there are no dangling references,
//! - accesses to mutable references are safe
//! - accesses to global storage references are safe

mod abstract_state;

use crate::absint::{FunctionContext, TransferFunctions, analyze_function};
use crate::regex_reference_safety::abstract_state::STEP_BASE_COST;
use abstract_state::{AbstractState, AbstractValue};
use move_abstract_stack::{AbsStackError, AbstractStack};
use move_binary_format::{
    CompiledModule,
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        Bytecode, CodeOffset, FunctionHandle, StructDefinition, StructFieldInformation,
        VariantDefinition,
    },
    safe_assert, safe_unwrap, safe_unwrap_err,
};
use move_bytecode_verifier_meter::{Meter, Scope};
use move_core_types::vm_status::StatusCode;
use std::num::NonZeroU64;

use self::abstract_state::ValueKind;

struct ReferenceSafetyAnalysis<'a> {
    module: &'a CompiledModule,
    function_context: &'a FunctionContext<'a>,
    stack: AbstractStack<AbstractValue>,
}

impl<'a> ReferenceSafetyAnalysis<'a> {
    fn new(module: &'a CompiledModule, function_context: &'a FunctionContext<'a>) -> Self {
        Self {
            module,
            function_context,
            stack: AbstractStack::new(),
        }
    }

    fn push(&mut self, v: AbstractValue) -> PartialVMResult<()> {
        safe_unwrap_err!(self.stack.push(v));
        Ok(())
    }

    fn push_n(&mut self, v: AbstractValue, n: u64) -> PartialVMResult<()> {
        safe_unwrap_err!(self.stack.push_n(v, n));
        Ok(())
    }
}

pub fn verify(
    module: &CompiledModule,
    function_context: &FunctionContext,
    meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    let initial_state = AbstractState::new(function_context)?;

    let mut verifier = ReferenceSafetyAnalysis::new(module, function_context);
    analyze_function(function_context, meter, &mut verifier, initial_state)
}

fn call(
    verifier: &mut ReferenceSafetyAnalysis,
    state: &mut AbstractState,
    offset: CodeOffset,
    function_handle: &FunctionHandle,
    meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    let parameters = verifier.module.signature_at(function_handle.parameters);
    let arguments_opt = parameters
        .0
        .iter()
        .map(|_| verifier.stack.pop())
        .rev()
        .collect::<Result<Vec<AbstractValue>, AbsStackError>>();
    let arguments = safe_unwrap_err!(arguments_opt);

    let return_ = verifier.module.signature_at(function_handle.return_);
    let return_kinds = ValueKind::for_signature(return_);
    let values = state.call(
        offset,
        arguments,
        &return_kinds,
        meter,
        StatusCode::CALL_BORROWED_MUTABLE_REFERENCE_ERROR,
    )?;
    for value in values {
        verifier.push(value)?
    }
    Ok(())
}

fn num_fields(struct_def: &StructDefinition) -> usize {
    match &struct_def.field_information {
        StructFieldInformation::Native => 0,
        StructFieldInformation::Declared(fields) => fields.len(),
    }
}

fn pack_struct(
    verifier: &mut ReferenceSafetyAnalysis,
    struct_def: &StructDefinition,
) -> PartialVMResult<()> {
    for _ in 0..num_fields(struct_def) {
        safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref())
    }
    // TODO maybe call state.value_for
    verifier.push(AbstractValue::NonReference)?;
    Ok(())
}

fn unpack_struct(
    verifier: &mut ReferenceSafetyAnalysis,
    struct_def: &StructDefinition,
) -> PartialVMResult<()> {
    safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
    // TODO maybe call state.value_for
    verifier.push_n(AbstractValue::NonReference, num_fields(struct_def) as u64)?;
    Ok(())
}

fn pack_enum_variant(
    verifier: &mut ReferenceSafetyAnalysis,
    variant_def: &VariantDefinition,
) -> PartialVMResult<()> {
    for _ in 0..variant_def.fields.len() {
        safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref())
    }
    // TODO maybe call state.value_for
    verifier.push(AbstractValue::NonReference)?;
    Ok(())
}

fn unpack_enum_variant(
    verifier: &mut ReferenceSafetyAnalysis,
    variant_def: &VariantDefinition,
) -> PartialVMResult<()> {
    safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
    // TODO maybe call state.value_for
    verifier.push_n(AbstractValue::NonReference, variant_def.fields.len() as u64)?;
    Ok(())
}

fn execute_inner(
    verifier: &mut ReferenceSafetyAnalysis,
    state: &mut AbstractState,
    bytecode: &Bytecode,
    offset: CodeOffset,
    meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    meter.add(Scope::Function, STEP_BASE_COST)?;

    match bytecode {
        Bytecode::Pop => state.release_value(safe_unwrap_err!(verifier.stack.pop()))?,

        Bytecode::CopyLoc(local) => {
            let value = state.copy_loc(offset, *local, meter)?;
            verifier.push(value)?
        }
        Bytecode::MoveLoc(local) => {
            let value = state.move_loc(offset, *local, meter)?;
            verifier.push(value)?
        }
        Bytecode::StLoc(local) => state.st_loc(
            offset,
            *local,
            safe_unwrap_err!(verifier.stack.pop()),
            meter,
        )?,

        Bytecode::FreezeRef => {
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            let frozen = state.freeze_ref(offset, r, meter)?;
            verifier.push(frozen)?
        }
        Bytecode::Eq | Bytecode::Neq => {
            let v1 = safe_unwrap_err!(verifier.stack.pop());
            let v2 = safe_unwrap_err!(verifier.stack.pop());
            let value = state.comparison(offset, v1, v2)?;
            verifier.push(value)?
        }
        Bytecode::ReadRef => {
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            let value = state.read_ref(offset, r)?;
            verifier.push(value)?
        }
        Bytecode::WriteRef => {
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            let val_operand = safe_unwrap_err!(verifier.stack.pop());
            safe_assert!(val_operand.is_non_ref());
            state.write_ref(offset, r, meter)?
        }

        Bytecode::MutBorrowLoc(local) => {
            let value = state.borrow_loc(offset, true, *local, meter)?;
            verifier.push(value)?
        }
        Bytecode::ImmBorrowLoc(local) => {
            let value = state.borrow_loc(offset, false, *local, meter)?;
            verifier.push(value)?
        }
        Bytecode::MutBorrowField(field_handle_index) => {
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            let value = state.borrow_field(offset, true, r, *field_handle_index, meter)?;
            verifier.push(value)?
        }
        Bytecode::MutBorrowFieldGeneric(field_inst_index) => {
            let field_inst = verifier.module.field_instantiation_at(*field_inst_index);
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            let value = state.borrow_field(offset, true, r, field_inst.handle, meter)?;
            verifier.push(value)?
        }
        Bytecode::ImmBorrowField(field_handle_index) => {
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            let value = state.borrow_field(offset, false, r, *field_handle_index, meter)?;
            verifier.push(value)?
        }
        Bytecode::ImmBorrowFieldGeneric(field_inst_index) => {
            let field_inst = verifier.module.field_instantiation_at(*field_inst_index);
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            let value = state.borrow_field(offset, false, r, field_inst.handle, meter)?;
            verifier.push(value)?
        }

        Bytecode::Call(idx) => {
            let function_handle = verifier.module.function_handle_at(*idx);
            call(verifier, state, offset, function_handle, meter)?
        }
        Bytecode::CallGeneric(idx) => {
            let func_inst = verifier.module.function_instantiation_at(*idx);
            let function_handle = verifier.module.function_handle_at(func_inst.handle);
            call(verifier, state, offset, function_handle, meter)?
        }

        Bytecode::Ret => {
            let mut return_values = vec![];
            for _ in 0..verifier.function_context.return_().len() {
                return_values.push(safe_unwrap_err!(verifier.stack.pop()));
            }
            return_values.reverse();

            state.ret(offset, return_values, meter)?
        }

        Bytecode::Branch(_) | Bytecode::Nop => (),

        Bytecode::CastU8
        | Bytecode::CastU16
        | Bytecode::CastU32
        | Bytecode::CastU64
        | Bytecode::CastU128
        | Bytecode::CastU256
        | Bytecode::Not => {
            safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
            verifier.push(AbstractValue::NonReference)?
        }

        Bytecode::BrTrue(_) | Bytecode::BrFalse(_) => {
            safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
        }

        Bytecode::Abort => {
            safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
            state.abort()
        }
        Bytecode::LdTrue
        | Bytecode::LdFalse
        | Bytecode::LdU8(_)
        | Bytecode::LdU16(_)
        | Bytecode::LdU32(_)
        | Bytecode::LdU64(_)
        | Bytecode::LdU128(_)
        | Bytecode::LdU256(_)
        | Bytecode::LdConst(_) => verifier.push(AbstractValue::NonReference)?,

        Bytecode::Add
        | Bytecode::Sub
        | Bytecode::Mul
        | Bytecode::Mod
        | Bytecode::Div
        | Bytecode::BitOr
        | Bytecode::BitAnd
        | Bytecode::Xor
        | Bytecode::Shl
        | Bytecode::Shr
        | Bytecode::Or
        | Bytecode::And
        | Bytecode::Lt
        | Bytecode::Gt
        | Bytecode::Le
        | Bytecode::Ge => {
            safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
            safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
            // TODO maybe call state.value_for
            verifier.push(AbstractValue::NonReference)?
        }

        Bytecode::Pack(idx) => {
            let struct_def = verifier.module.struct_def_at(*idx);
            pack_struct(verifier, struct_def)?
        }
        Bytecode::PackGeneric(idx) => {
            let struct_inst = verifier.module.struct_instantiation_at(*idx);
            let struct_def = verifier.module.struct_def_at(struct_inst.def);
            pack_struct(verifier, struct_def)?
        }
        Bytecode::Unpack(idx) => {
            let struct_def = verifier.module.struct_def_at(*idx);
            unpack_struct(verifier, struct_def)?
        }
        Bytecode::UnpackGeneric(idx) => {
            let struct_inst = verifier.module.struct_instantiation_at(*idx);
            let struct_def = verifier.module.struct_def_at(struct_inst.def);
            unpack_struct(verifier, struct_def)?
        }

        Bytecode::VecPack(_, num) => {
            if let Some(num_to_pop) = NonZeroU64::new(*num) {
                let result = verifier.stack.pop_eq_n(num_to_pop);
                let abs_value = safe_unwrap_err!(result);
                safe_assert!(abs_value.is_non_ref());
            }

            verifier.push(AbstractValue::NonReference)?;
        }

        Bytecode::VecLen(_) => {
            let vec_ref = safe_unwrap_err!(verifier.stack.pop());
            state.vector_op(offset, vec_ref, false, meter)?;
            verifier.push(AbstractValue::NonReference)?;
        }

        Bytecode::VecImmBorrow(_) => {
            safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
            let vec_ref = safe_unwrap_err!(verifier.stack.pop());
            let values = state.call(
                offset,
                vec![vec_ref],
                &[ValueKind::Reference(false)],
                meter,
                StatusCode::VEC_BORROW_ELEMENT_EXISTS_MUTABLE_BORROW_ERROR, // should not be hit
            )?;
            debug_assert!(values.len() == 1);
            for value in values {
                verifier.push(value)?
            }
        }
        Bytecode::VecMutBorrow(_) => {
            safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
            let vec_ref = safe_unwrap_err!(verifier.stack.pop());
            let values = state.call(
                offset,
                vec![vec_ref],
                &[ValueKind::Reference(true)],
                meter,
                StatusCode::VEC_BORROW_ELEMENT_EXISTS_MUTABLE_BORROW_ERROR,
            )?;
            debug_assert!(values.len() == 1);
            for value in values {
                verifier.push(value)?
            }
        }

        Bytecode::VecPushBack(_) => {
            safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
            let vec_ref = safe_unwrap_err!(verifier.stack.pop());
            state.vector_op(offset, vec_ref, true, meter)?;
        }

        Bytecode::VecPopBack(_) => {
            let vec_ref = safe_unwrap_err!(verifier.stack.pop());
            state.vector_op(offset, vec_ref, true, meter)?;

            verifier.push(AbstractValue::NonReference)?
        }

        Bytecode::VecUnpack(_, num) => {
            safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());

            verifier.push_n(AbstractValue::NonReference, *num)?
        }

        Bytecode::VecSwap(_) => {
            safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
            safe_assert!(safe_unwrap_err!(verifier.stack.pop()).is_non_ref());
            let vec_ref = safe_unwrap_err!(verifier.stack.pop());
            state.vector_op(offset, vec_ref, true, meter)?;
        }
        Bytecode::PackVariant(vidx) => {
            let handle = verifier.module.variant_handle_at(*vidx);
            let variant_def = verifier
                .module
                .variant_def_at(handle.enum_def, handle.variant);
            pack_enum_variant(verifier, variant_def)?
        }
        Bytecode::PackVariantGeneric(vidx) => {
            let handle = verifier.module.variant_instantiation_handle_at(*vidx);
            let enum_def = verifier.module.enum_instantiation_at(handle.enum_def);
            let variant_def = verifier.module.variant_def_at(enum_def.def, handle.variant);
            pack_enum_variant(verifier, variant_def)?
        }
        Bytecode::UnpackVariant(vidx) => {
            let handle = verifier.module.variant_handle_at(*vidx);
            let variant_def = verifier
                .module
                .variant_def_at(handle.enum_def, handle.variant);
            unpack_enum_variant(verifier, variant_def)?
        }
        Bytecode::UnpackVariantGeneric(vidx) => {
            let handle = verifier.module.variant_instantiation_handle_at(*vidx);
            let enum_def = verifier.module.enum_instantiation_at(handle.enum_def);
            let variant_def = verifier.module.variant_def_at(enum_def.def, handle.variant);
            unpack_enum_variant(verifier, variant_def)?
        }
        Bytecode::UnpackVariantImmRef(vidx) => {
            let handle = verifier.module.variant_handle_at(*vidx);
            let variant_def = verifier
                .module
                .variant_def_at(handle.enum_def, handle.variant);
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            for val in state
                .unpack_enum_variant_ref(
                    offset,
                    handle.enum_def,
                    handle.variant,
                    variant_def,
                    false,
                    r,
                    meter,
                )?
                .into_iter()
            {
                verifier.push(val)?
            }
        }
        Bytecode::UnpackVariantMutRef(vidx) => {
            let handle = verifier.module.variant_handle_at(*vidx);
            let variant_def = verifier
                .module
                .variant_def_at(handle.enum_def, handle.variant);
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            for val in state
                .unpack_enum_variant_ref(
                    offset,
                    handle.enum_def,
                    handle.variant,
                    variant_def,
                    true,
                    r,
                    meter,
                )?
                .into_iter()
            {
                verifier.push(val)?
            }
        }
        Bytecode::UnpackVariantGenericImmRef(vidx) => {
            let handle = verifier.module.variant_instantiation_handle_at(*vidx);
            let enum_def = verifier.module.enum_instantiation_at(handle.enum_def);
            let variant_def = verifier.module.variant_def_at(enum_def.def, handle.variant);
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            for val in state
                .unpack_enum_variant_ref(
                    offset,
                    enum_def.def,
                    handle.variant,
                    variant_def,
                    false,
                    r,
                    meter,
                )?
                .into_iter()
            {
                verifier.push(val)?
            }
        }
        Bytecode::UnpackVariantGenericMutRef(vidx) => {
            let handle = verifier.module.variant_instantiation_handle_at(*vidx);
            let enum_def = verifier.module.enum_instantiation_at(handle.enum_def);
            let variant_def = verifier.module.variant_def_at(enum_def.def, handle.variant);
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            for val in state
                .unpack_enum_variant_ref(
                    offset,
                    enum_def.def,
                    handle.variant,
                    variant_def,
                    true,
                    r,
                    meter,
                )?
                .into_iter()
            {
                verifier.push(val)?
            }
        }
        Bytecode::VariantSwitch(_) => {
            let r = safe_unwrap!(safe_unwrap_err!(verifier.stack.pop()).to_ref());
            state.read_ref(offset, r)?;
        }

        Bytecode::ExistsDeprecated(_)
        | Bytecode::ExistsGenericDeprecated(_)
        | Bytecode::MoveFromDeprecated(_)
        | Bytecode::MoveFromGenericDeprecated(_)
        | Bytecode::MoveToDeprecated(_)
        | Bytecode::MoveToGenericDeprecated(_)
        | Bytecode::MutBorrowGlobalDeprecated(_)
        | Bytecode::MutBorrowGlobalGenericDeprecated(_)
        | Bytecode::ImmBorrowGlobalDeprecated(_)
        | Bytecode::ImmBorrowGlobalGenericDeprecated(_) => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Unsupported deprecated bytecode".to_string()),
            );
        }
    };
    Ok(())
}

impl TransferFunctions for ReferenceSafetyAnalysis<'_> {
    type State = AbstractState;

    fn execute(
        &mut self,
        state: &mut Self::State,
        bytecode: &Bytecode,
        index: CodeOffset,
        (first_index, last_index): (CodeOffset, CodeOffset),
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        if index == first_index {
            safe_assert!(self.stack.is_empty());
            state.refresh()?
        }
        execute_inner(self, state, bytecode, index, meter)?;
        #[cfg(debug_assertions)]
        state.check_invariants();
        if index == last_index {
            safe_assert!(self.stack.is_empty());
            state.canonicalize()?
        }
        Ok(())
    }
}
