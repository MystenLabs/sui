// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines the transfer functions for verifying type safety of a procedure body.
//! It does not utilize control flow, but does check each block independently

use std::num::NonZeroU64;

use crate::absint::FunctionContext;
use move_abstract_interpreter::control_flow_graph::ControlFlowGraph;
use move_abstract_stack::AbstractStack;
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        AbilitySet, Bytecode, CodeOffset, CompiledModule, DatatypeHandleIndex, FieldHandleIndex,
        FunctionDefinitionIndex, FunctionHandle, LocalIndex, Signature, SignatureToken,
        SignatureToken as ST, StructDefinition, StructDefinitionIndex, StructFieldInformation,
    },
    safe_unwrap_err,
};
use move_bytecode_verifier_meter::{Meter, Scope};
use move_core_types::vm_status::StatusCode;

struct Locals<'a> {
    param_count: usize,
    parameters: &'a Signature,
    locals: &'a Signature,
}

const TYPE_NODE_COST: u128 = 30;

impl<'a> Locals<'a> {
    fn new(parameters: &'a Signature, locals: &'a Signature) -> Self {
        Self {
            param_count: parameters.len(),
            parameters,
            locals,
        }
    }

    fn local_at(&self, i: LocalIndex) -> &SignatureToken {
        let idx = i as usize;
        if idx < self.param_count {
            &self.parameters.0[idx]
        } else {
            &self.locals.0[idx - self.param_count]
        }
    }
}

struct TypeSafetyChecker<'a> {
    resolver: &'a CompiledModule,
    function_context: &'a FunctionContext<'a>,
    locals: Locals<'a>,
    stack: AbstractStack<SignatureToken>,
}

impl<'a> TypeSafetyChecker<'a> {
    fn new(resolver: &'a CompiledModule, function_context: &'a FunctionContext<'a>) -> Self {
        let locals = Locals::new(function_context.parameters(), function_context.locals());
        Self {
            resolver,
            function_context,
            locals,
            stack: AbstractStack::new(),
        }
    }

    fn local_at(&self, i: LocalIndex) -> &SignatureToken {
        self.locals.local_at(i)
    }

    fn abilities(&self, t: &SignatureToken) -> PartialVMResult<AbilitySet> {
        self.resolver
            .abilities(t, self.function_context.type_parameters())
    }

    fn error(&self, status: StatusCode, offset: CodeOffset) -> PartialVMError {
        PartialVMError::new(status).at_code_offset(
            self.function_context
                .index()
                .unwrap_or(FunctionDefinitionIndex(0)),
            offset,
        )
    }

    fn push(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        ty: SignatureToken,
    ) -> PartialVMResult<()> {
        self.charge_ty(meter, &ty)?;
        safe_unwrap_err!(self.stack.push(ty));
        Ok(())
    }

    fn push_n(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        ty: SignatureToken,
        n: u64,
    ) -> PartialVMResult<()> {
        self.charge_ty(meter, &ty)?;
        safe_unwrap_err!(self.stack.push_n(ty, n));
        Ok(())
    }

    fn charge_ty(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        ty: &SignatureToken,
    ) -> PartialVMResult<()> {
        self.charge_ty_(meter, ty, 1)
    }

    fn charge_ty_(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        ty: &SignatureToken,
        n: u64,
    ) -> PartialVMResult<()> {
        meter.add_items(
            Scope::Function,
            TYPE_NODE_COST,
            ty.preorder_traversal().count() * (n as usize),
        )
    }

    fn charge_tys(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        tys: &[SignatureToken],
    ) -> PartialVMResult<()> {
        for ty in tys {
            self.charge_ty(meter, ty)?
        }
        Ok(())
    }
}

pub(crate) fn verify<'a>(
    resolver: &'a CompiledModule,
    function_context: &'a FunctionContext<'a>,
    meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    let verifier = &mut TypeSafetyChecker::new(resolver, function_context);

    for block_id in function_context.cfg().blocks() {
        for offset in function_context.cfg().instr_indexes(block_id) {
            let instr = &verifier.function_context.code().code[offset as usize];
            verify_instr(verifier, instr, offset, meter)?
        }
    }

    Ok(())
}

// helper for both `ImmBorrowField` and `MutBorrowField`
fn borrow_field(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    mut_: bool,
    field_handle_index: FieldHandleIndex,
    type_args: &Signature,
) -> PartialVMResult<()> {
    // load operand and check mutability constraints
    let operand = safe_unwrap_err!(verifier.stack.pop());
    if mut_ && !operand.is_mutable_reference() {
        return Err(verifier.error(StatusCode::BORROWFIELD_TYPE_MISMATCH_ERROR, offset));
    }

    // check the reference on the stack is the expected type.
    // Load the type that owns the field according to the instruction.
    // For generic fields access, this step materializes that type
    let field_handle = verifier.resolver.field_handle_at(field_handle_index);
    let struct_def = verifier.resolver.struct_def_at(field_handle.owner);
    let expected_type = materialize_type(struct_def.struct_handle, type_args);
    match operand {
        ST::Reference(inner) | ST::MutableReference(inner) if expected_type == *inner => (),
        _ => return Err(verifier.error(StatusCode::BORROWFIELD_TYPE_MISMATCH_ERROR, offset)),
    }

    let field_def = match &struct_def.field_information {
        StructFieldInformation::Native => {
            return Err(verifier.error(StatusCode::BORROWFIELD_BAD_FIELD_ERROR, offset));
        }
        StructFieldInformation::Declared(fields) => {
            // TODO: review the whole error story here, way too much is left to chances...
            // definition of a more proper OM for the verifier could work around the problem
            // (maybe, maybe not..)
            &fields[field_handle.field as usize]
        }
    };
    let field_type = Box::new(instantiate(&field_def.signature.0, type_args));
    verifier.push(
        meter,
        if mut_ {
            ST::MutableReference(field_type)
        } else {
            ST::Reference(field_type)
        },
    )?;
    Ok(())
}

// helper for both `ImmBorrowLoc` and `MutBorrowLoc`
fn borrow_loc(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    mut_: bool,
    idx: LocalIndex,
) -> PartialVMResult<()> {
    let loc_signature = verifier.local_at(idx).clone();

    if loc_signature.is_reference() {
        return Err(verifier.error(StatusCode::BORROWLOC_REFERENCE_ERROR, offset));
    }

    verifier.push(
        meter,
        if mut_ {
            ST::MutableReference(Box::new(loc_signature))
        } else {
            ST::Reference(Box::new(loc_signature))
        },
    )?;
    Ok(())
}

fn borrow_global(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    mut_: bool,
    idx: StructDefinitionIndex,
    type_args: &Signature,
) -> PartialVMResult<()> {
    // check and consume top of stack
    let operand = safe_unwrap_err!(verifier.stack.pop());
    if operand != ST::Address {
        return Err(verifier.error(StatusCode::BORROWGLOBAL_TYPE_MISMATCH_ERROR, offset));
    }

    let struct_def = verifier.resolver.struct_def_at(idx);
    let struct_type = materialize_type(struct_def.struct_handle, type_args);
    if !verifier.abilities(&struct_type)?.has_key() {
        return Err(verifier.error(StatusCode::BORROWGLOBAL_WITHOUT_KEY_ABILITY, offset));
    }

    let struct_type = materialize_type(struct_def.struct_handle, type_args);
    verifier.push(
        meter,
        if mut_ {
            ST::MutableReference(Box::new(struct_type))
        } else {
            ST::Reference(Box::new(struct_type))
        },
    )?;
    Ok(())
}

fn call(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    function_handle: &FunctionHandle,
    type_actuals: &Signature,
) -> PartialVMResult<()> {
    let parameters = verifier.resolver.signature_at(function_handle.parameters);
    for parameter in parameters.0.iter().rev() {
        let arg = safe_unwrap_err!(verifier.stack.pop());
        if (type_actuals.is_empty() && &arg != parameter)
            || (!type_actuals.is_empty() && arg != instantiate(parameter, type_actuals))
        {
            return Err(verifier.error(StatusCode::CALL_TYPE_MISMATCH_ERROR, offset));
        }
    }
    for return_type in &verifier.resolver.signature_at(function_handle.return_).0 {
        verifier.push(meter, instantiate(return_type, type_actuals))?
    }
    Ok(())
}

fn type_fields_signature(
    verifier: &mut TypeSafetyChecker,
    _meter: &mut (impl Meter + ?Sized), // TODO: metering
    offset: CodeOffset,
    struct_def: &StructDefinition,
    type_args: &Signature,
) -> PartialVMResult<Signature> {
    match &struct_def.field_information {
        StructFieldInformation::Native => {
            // TODO: this is more of "unreachable"
            Err(verifier.error(StatusCode::PACK_TYPE_MISMATCH_ERROR, offset))
        }
        StructFieldInformation::Declared(fields) => {
            let mut field_sig = vec![];
            for field_def in fields.iter() {
                field_sig.push(instantiate(&field_def.signature.0, type_args));
            }
            Ok(Signature(field_sig))
        }
    }
}

fn pack(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    struct_def: &StructDefinition,
    type_args: &Signature,
) -> PartialVMResult<()> {
    let struct_type = materialize_type(struct_def.struct_handle, type_args);
    let field_sig = type_fields_signature(verifier, meter, offset, struct_def, type_args)?;
    for sig in field_sig.0.iter().rev() {
        let arg = safe_unwrap_err!(verifier.stack.pop());
        if &arg != sig {
            return Err(verifier.error(StatusCode::PACK_TYPE_MISMATCH_ERROR, offset));
        }
    }

    verifier.push(meter, struct_type)?;
    Ok(())
}

fn unpack(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    struct_def: &StructDefinition,
    type_args: &Signature,
) -> PartialVMResult<()> {
    let struct_type = materialize_type(struct_def.struct_handle, type_args);

    // Pop an abstract value from the stack and check if its type is equal to the one
    // declared.
    let arg = safe_unwrap_err!(verifier.stack.pop());
    if arg != struct_type {
        return Err(verifier.error(StatusCode::UNPACK_TYPE_MISMATCH_ERROR, offset));
    }

    let field_sig = type_fields_signature(verifier, meter, offset, struct_def, type_args)?;
    for sig in field_sig.0 {
        verifier.push(meter, sig)?
    }
    Ok(())
}

fn exists(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    struct_def: &StructDefinition,
    type_args: &Signature,
) -> PartialVMResult<()> {
    let struct_type = materialize_type(struct_def.struct_handle, type_args);
    if !verifier.abilities(&struct_type)?.has_key() {
        return Err(verifier.error(
            StatusCode::EXISTS_WITHOUT_KEY_ABILITY_OR_BAD_ARGUMENT,
            offset,
        ));
    }

    let operand = safe_unwrap_err!(verifier.stack.pop());
    if operand != ST::Address {
        // TODO better error here
        return Err(verifier.error(
            StatusCode::EXISTS_WITHOUT_KEY_ABILITY_OR_BAD_ARGUMENT,
            offset,
        ));
    }

    verifier.push(meter, ST::Bool)?;
    Ok(())
}

fn move_from(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    struct_def: &StructDefinition,
    type_args: &Signature,
) -> PartialVMResult<()> {
    let struct_type = materialize_type(struct_def.struct_handle, type_args);
    if !verifier.abilities(&struct_type)?.has_key() {
        return Err(verifier.error(StatusCode::MOVEFROM_WITHOUT_KEY_ABILITY, offset));
    }

    let struct_type = materialize_type(struct_def.struct_handle, type_args);
    let operand = safe_unwrap_err!(verifier.stack.pop());
    if operand != ST::Address {
        return Err(verifier.error(StatusCode::MOVEFROM_TYPE_MISMATCH_ERROR, offset));
    }

    verifier.push(meter, struct_type)?;
    Ok(())
}

fn move_to(
    verifier: &mut TypeSafetyChecker,
    offset: CodeOffset,
    struct_def: &StructDefinition,
    type_args: &Signature,
) -> PartialVMResult<()> {
    let struct_type = materialize_type(struct_def.struct_handle, type_args);
    if !verifier.abilities(&struct_type)?.has_key() {
        return Err(verifier.error(StatusCode::MOVETO_WITHOUT_KEY_ABILITY, offset));
    }

    let struct_type = materialize_type(struct_def.struct_handle, type_args);
    let key_struct_operand = safe_unwrap_err!(verifier.stack.pop());
    let signer_reference_operand = safe_unwrap_err!(verifier.stack.pop());
    if key_struct_operand != struct_type {
        return Err(verifier.error(StatusCode::MOVETO_TYPE_MISMATCH_ERROR, offset));
    }
    match signer_reference_operand {
        ST::Reference(inner) => match *inner {
            ST::Signer => Ok(()),
            _ => Err(verifier.error(StatusCode::MOVETO_TYPE_MISMATCH_ERROR, offset)),
        },
        _ => Err(verifier.error(StatusCode::MOVETO_TYPE_MISMATCH_ERROR, offset)),
    }
}

fn borrow_vector_element(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    declared_element_type: &SignatureToken,
    offset: CodeOffset,
    mut_ref_only: bool,
) -> PartialVMResult<()> {
    let operand_idx = safe_unwrap_err!(verifier.stack.pop());
    let operand_vec = safe_unwrap_err!(verifier.stack.pop());

    // check index
    if operand_idx != ST::U64 {
        return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset));
    }

    // check vector and update stack
    let element_type = match get_vector_element_type(operand_vec, mut_ref_only) {
        Some(ty) if &ty == declared_element_type => ty,
        _ => return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset)),
    };
    let element_ref_type = if mut_ref_only {
        ST::MutableReference(Box::new(element_type))
    } else {
        ST::Reference(Box::new(element_type))
    };
    verifier.push(meter, element_ref_type)?;

    Ok(())
}

fn verify_instr(
    verifier: &mut TypeSafetyChecker,
    bytecode: &Bytecode,
    offset: CodeOffset,
    meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    match bytecode {
        Bytecode::Pop => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            let abilities = verifier
                .resolver
                .abilities(&operand, verifier.function_context.type_parameters());
            if !abilities?.has_drop() {
                return Err(verifier.error(StatusCode::POP_WITHOUT_DROP_ABILITY, offset));
            }
        }

        Bytecode::BrTrue(_) | Bytecode::BrFalse(_) => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            if operand != ST::Bool {
                return Err(verifier.error(StatusCode::BR_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::StLoc(idx) => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            if &operand != verifier.local_at(*idx) {
                return Err(verifier.error(StatusCode::STLOC_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Abort => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            if operand != ST::U64 {
                return Err(verifier.error(StatusCode::ABORT_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Ret => {
            let return_ = &verifier.function_context.return_().0;
            for return_type in return_.iter().rev() {
                let operand = safe_unwrap_err!(verifier.stack.pop());
                if &operand != return_type {
                    return Err(verifier.error(StatusCode::RET_TYPE_MISMATCH_ERROR, offset));
                }
            }
        }

        Bytecode::Branch(_) | Bytecode::Nop => (),

        Bytecode::FreezeRef => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            match operand {
                ST::MutableReference(inner) => verifier.push(meter, ST::Reference(inner))?,
                _ => return Err(verifier.error(StatusCode::FREEZEREF_TYPE_MISMATCH_ERROR, offset)),
            }
        }

        Bytecode::MutBorrowField(field_handle_index) => borrow_field(
            verifier,
            meter,
            offset,
            true,
            *field_handle_index,
            &Signature(vec![]),
        )?,

        Bytecode::MutBorrowFieldGeneric(field_inst_index) => {
            let field_inst = verifier.resolver.field_instantiation_at(*field_inst_index);
            let type_inst = verifier.resolver.signature_at(field_inst.type_parameters);
            verifier.charge_tys(meter, &type_inst.0)?;
            borrow_field(verifier, meter, offset, true, field_inst.handle, type_inst)?
        }

        Bytecode::ImmBorrowField(field_handle_index) => borrow_field(
            verifier,
            meter,
            offset,
            false,
            *field_handle_index,
            &Signature(vec![]),
        )?,

        Bytecode::ImmBorrowFieldGeneric(field_inst_index) => {
            let field_inst = verifier.resolver.field_instantiation_at(*field_inst_index);
            let type_inst = verifier.resolver.signature_at(field_inst.type_parameters);
            verifier.charge_tys(meter, &type_inst.0)?;
            borrow_field(verifier, meter, offset, false, field_inst.handle, type_inst)?
        }

        Bytecode::LdU8(_) => {
            verifier.push(meter, ST::U8)?;
        }

        Bytecode::LdU16(_) => {
            verifier.push(meter, ST::U16)?;
        }

        Bytecode::LdU32(_) => {
            verifier.push(meter, ST::U32)?;
        }

        Bytecode::LdU64(_) => {
            verifier.push(meter, ST::U64)?;
        }

        Bytecode::LdU128(_) => {
            verifier.push(meter, ST::U128)?;
        }

        Bytecode::LdU256(_) => {
            verifier.push(meter, ST::U256)?;
        }

        Bytecode::LdConst(idx) => {
            let signature = verifier.resolver.constant_at(*idx).type_.clone();
            verifier.push(meter, signature)?;
        }

        Bytecode::LdTrue | Bytecode::LdFalse => {
            verifier.push(meter, ST::Bool)?;
        }

        Bytecode::CopyLoc(idx) => {
            let local_signature = verifier.local_at(*idx).clone();
            if !verifier
                .resolver
                .abilities(
                    &local_signature,
                    verifier.function_context.type_parameters(),
                )?
                .has_copy()
            {
                return Err(verifier.error(StatusCode::COPYLOC_WITHOUT_COPY_ABILITY, offset));
            }
            verifier.push(meter, local_signature)?
        }

        Bytecode::MoveLoc(idx) => {
            let local_signature = verifier.local_at(*idx).clone();
            verifier.push(meter, local_signature)?
        }

        Bytecode::MutBorrowLoc(idx) => borrow_loc(verifier, meter, offset, true, *idx)?,

        Bytecode::ImmBorrowLoc(idx) => borrow_loc(verifier, meter, offset, false, *idx)?,

        Bytecode::Call(idx) => {
            let function_handle = verifier.resolver.function_handle_at(*idx);
            call(verifier, meter, offset, function_handle, &Signature(vec![]))?
        }

        Bytecode::CallGeneric(idx) => {
            let func_inst = verifier.resolver.function_instantiation_at(*idx);
            let func_handle = verifier.resolver.function_handle_at(func_inst.handle);
            let type_args = &verifier.resolver.signature_at(func_inst.type_parameters);
            verifier.charge_tys(meter, &type_args.0)?;
            call(verifier, meter, offset, func_handle, type_args)?
        }

        Bytecode::Pack(idx) => {
            let struct_definition = verifier.resolver.struct_def_at(*idx);
            pack(
                verifier,
                meter,
                offset,
                struct_definition,
                &Signature(vec![]),
            )?
        }

        Bytecode::PackGeneric(idx) => {
            let struct_inst = verifier.resolver.struct_instantiation_at(*idx);
            let struct_def = verifier.resolver.struct_def_at(struct_inst.def);
            let type_args = verifier.resolver.signature_at(struct_inst.type_parameters);
            verifier.charge_tys(meter, &type_args.0)?;
            pack(verifier, meter, offset, struct_def, type_args)?
        }

        Bytecode::Unpack(idx) => {
            let struct_definition = verifier.resolver.struct_def_at(*idx);
            unpack(
                verifier,
                meter,
                offset,
                struct_definition,
                &Signature(vec![]),
            )?
        }

        Bytecode::UnpackGeneric(idx) => {
            let struct_inst = verifier.resolver.struct_instantiation_at(*idx);
            let struct_def = verifier.resolver.struct_def_at(struct_inst.def);
            let type_args = verifier.resolver.signature_at(struct_inst.type_parameters);
            verifier.charge_tys(meter, &type_args.0)?;
            unpack(verifier, meter, offset, struct_def, type_args)?
        }

        Bytecode::ReadRef => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            match operand {
                ST::Reference(inner) | ST::MutableReference(inner) => {
                    if !verifier.abilities(&inner)?.has_copy() {
                        return Err(
                            verifier.error(StatusCode::READREF_WITHOUT_COPY_ABILITY, offset)
                        );
                    }
                    verifier.push(meter, *inner)?;
                }
                _ => return Err(verifier.error(StatusCode::READREF_TYPE_MISMATCH_ERROR, offset)),
            }
        }

        Bytecode::WriteRef => {
            let ref_operand = safe_unwrap_err!(verifier.stack.pop());
            let val_operand = safe_unwrap_err!(verifier.stack.pop());
            let ref_inner_signature = match ref_operand {
                ST::MutableReference(inner) => *inner,
                _ => {
                    return Err(
                        verifier.error(StatusCode::WRITEREF_NO_MUTABLE_REFERENCE_ERROR, offset)
                    )
                }
            };
            if !verifier.abilities(&ref_inner_signature)?.has_drop() {
                return Err(verifier.error(StatusCode::WRITEREF_WITHOUT_DROP_ABILITY, offset));
            }

            if val_operand != ref_inner_signature {
                return Err(verifier.error(StatusCode::WRITEREF_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::CastU8 => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            if !operand.is_integer() {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
            verifier.push(meter, ST::U8)?;
        }
        Bytecode::CastU64 => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            if !operand.is_integer() {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
            verifier.push(meter, ST::U64)?;
        }
        Bytecode::CastU128 => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            if !operand.is_integer() {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
            verifier.push(meter, ST::U128)?;
        }

        Bytecode::Add
        | Bytecode::Sub
        | Bytecode::Mul
        | Bytecode::Mod
        | Bytecode::Div
        | Bytecode::BitOr
        | Bytecode::BitAnd
        | Bytecode::Xor => {
            let operand1 = safe_unwrap_err!(verifier.stack.pop());
            let operand2 = safe_unwrap_err!(verifier.stack.pop());
            if operand1.is_integer() && operand1 == operand2 {
                verifier.push(meter, operand1)?;
            } else {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Shl | Bytecode::Shr => {
            let operand1 = safe_unwrap_err!(verifier.stack.pop());
            let operand2 = safe_unwrap_err!(verifier.stack.pop());
            if operand2.is_integer() && operand1 == ST::U8 {
                verifier.push(meter, operand2)?;
            } else {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Or | Bytecode::And => {
            let operand1 = safe_unwrap_err!(verifier.stack.pop());
            let operand2 = safe_unwrap_err!(verifier.stack.pop());
            if operand1 == ST::Bool && operand2 == ST::Bool {
                verifier.push(meter, ST::Bool)?;
            } else {
                return Err(verifier.error(StatusCode::BOOLEAN_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Not => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            if operand == ST::Bool {
                verifier.push(meter, ST::Bool)?;
            } else {
                return Err(verifier.error(StatusCode::BOOLEAN_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Eq | Bytecode::Neq => {
            let operand1 = safe_unwrap_err!(verifier.stack.pop());
            let operand2 = safe_unwrap_err!(verifier.stack.pop());
            if verifier.abilities(&operand1)?.has_drop() && operand1 == operand2 {
                verifier.push(meter, ST::Bool)?;
            } else {
                return Err(verifier.error(StatusCode::EQUALITY_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Lt | Bytecode::Gt | Bytecode::Le | Bytecode::Ge => {
            let operand1 = safe_unwrap_err!(verifier.stack.pop());
            let operand2 = safe_unwrap_err!(verifier.stack.pop());
            if operand1.is_integer() && operand1 == operand2 {
                verifier.push(meter, ST::Bool)?
            } else {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::MutBorrowGlobalDeprecated(idx) => {
            borrow_global(verifier, meter, offset, true, *idx, &Signature(vec![]))?
        }

        Bytecode::MutBorrowGlobalGenericDeprecated(idx) => {
            let struct_inst = verifier.resolver.struct_instantiation_at(*idx);
            let type_inst = verifier.resolver.signature_at(struct_inst.type_parameters);
            verifier.charge_tys(meter, &type_inst.0)?;
            borrow_global(verifier, meter, offset, true, struct_inst.def, type_inst)?
        }

        Bytecode::ImmBorrowGlobalDeprecated(idx) => {
            borrow_global(verifier, meter, offset, false, *idx, &Signature(vec![]))?
        }

        Bytecode::ImmBorrowGlobalGenericDeprecated(idx) => {
            let struct_inst = verifier.resolver.struct_instantiation_at(*idx);
            let type_inst = verifier.resolver.signature_at(struct_inst.type_parameters);
            verifier.charge_tys(meter, &type_inst.0)?;
            borrow_global(verifier, meter, offset, false, struct_inst.def, type_inst)?
        }

        Bytecode::ExistsDeprecated(idx) => {
            let struct_def = verifier.resolver.struct_def_at(*idx);
            exists(verifier, meter, offset, struct_def, &Signature(vec![]))?
        }

        Bytecode::ExistsGenericDeprecated(idx) => {
            let struct_inst = verifier.resolver.struct_instantiation_at(*idx);
            let struct_def = verifier.resolver.struct_def_at(struct_inst.def);
            let type_args = verifier.resolver.signature_at(struct_inst.type_parameters);
            verifier.charge_tys(meter, &type_args.0)?;
            exists(verifier, meter, offset, struct_def, type_args)?
        }

        Bytecode::MoveFromDeprecated(idx) => {
            let struct_def = verifier.resolver.struct_def_at(*idx);
            move_from(verifier, meter, offset, struct_def, &Signature(vec![]))?
        }

        Bytecode::MoveFromGenericDeprecated(idx) => {
            let struct_inst = verifier.resolver.struct_instantiation_at(*idx);
            let struct_def = verifier.resolver.struct_def_at(struct_inst.def);
            let type_args = verifier.resolver.signature_at(struct_inst.type_parameters);
            verifier.charge_tys(meter, &type_args.0)?;
            move_from(verifier, meter, offset, struct_def, type_args)?
        }

        Bytecode::MoveToDeprecated(idx) => {
            let struct_def = verifier.resolver.struct_def_at(*idx);
            move_to(verifier, offset, struct_def, &Signature(vec![]))?
        }

        Bytecode::MoveToGenericDeprecated(idx) => {
            let struct_inst = verifier.resolver.struct_instantiation_at(*idx);
            let struct_def = verifier.resolver.struct_def_at(struct_inst.def);
            let type_args = verifier.resolver.signature_at(struct_inst.type_parameters);
            verifier.charge_tys(meter, &type_args.0)?;
            move_to(verifier, offset, struct_def, type_args)?
        }

        Bytecode::VecPack(idx, num) => {
            let element_type = &verifier.resolver.signature_at(*idx).0[0];
            if let Some(num_to_pop) = NonZeroU64::new(*num) {
                let is_mismatched = verifier
                    .stack
                    .pop_eq_n(num_to_pop)
                    .map(|t| element_type != &t)
                    .unwrap_or(true);
                if is_mismatched {
                    return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset));
                }
            }
            verifier.push(meter, ST::Vector(Box::new(element_type.clone())))?;
        }

        Bytecode::VecLen(idx) => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            let declared_element_type = &verifier.resolver.signature_at(*idx).0[0];
            match get_vector_element_type(operand, false) {
                Some(derived_element_type) if &derived_element_type == declared_element_type => {
                    verifier.push(meter, ST::U64)?;
                }
                _ => return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset)),
            };
        }

        Bytecode::VecImmBorrow(idx) => {
            let declared_element_type = &verifier.resolver.signature_at(*idx).0[0];
            borrow_vector_element(verifier, meter, declared_element_type, offset, false)?
        }
        Bytecode::VecMutBorrow(idx) => {
            let declared_element_type = &verifier.resolver.signature_at(*idx).0[0];
            borrow_vector_element(verifier, meter, declared_element_type, offset, true)?
        }

        Bytecode::VecPushBack(idx) => {
            let operand_elem = safe_unwrap_err!(verifier.stack.pop());
            let operand_vec = safe_unwrap_err!(verifier.stack.pop());
            let declared_element_type = &verifier.resolver.signature_at(*idx).0[0];
            if declared_element_type != &operand_elem {
                return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset));
            }
            match get_vector_element_type(operand_vec, true) {
                Some(derived_element_type) if &derived_element_type == declared_element_type => {}
                _ => return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset)),
            };
        }

        Bytecode::VecPopBack(idx) => {
            let operand_vec = safe_unwrap_err!(verifier.stack.pop());
            let declared_element_type = &verifier.resolver.signature_at(*idx).0[0];
            match get_vector_element_type(operand_vec, true) {
                Some(derived_element_type) if &derived_element_type == declared_element_type => {
                    verifier.push(meter, derived_element_type)?;
                }
                _ => return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset)),
            };
        }

        Bytecode::VecUnpack(idx, num) => {
            let operand_vec = safe_unwrap_err!(verifier.stack.pop());
            let declared_element_type = &verifier.resolver.signature_at(*idx).0[0];
            if operand_vec != ST::Vector(Box::new(declared_element_type.clone())) {
                return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset));
            }
            verifier.push_n(meter, declared_element_type.clone(), *num)?;
        }

        Bytecode::VecSwap(idx) => {
            let operand_idx2 = safe_unwrap_err!(verifier.stack.pop());
            let operand_idx1 = safe_unwrap_err!(verifier.stack.pop());
            let operand_vec = safe_unwrap_err!(verifier.stack.pop());
            if operand_idx1 != ST::U64 || operand_idx2 != ST::U64 {
                return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset));
            }
            let declared_element_type = &verifier.resolver.signature_at(*idx).0[0];
            match get_vector_element_type(operand_vec, true) {
                Some(derived_element_type) if &derived_element_type == declared_element_type => {}
                _ => return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset)),
            };
        }
        Bytecode::CastU16 => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            if !operand.is_integer() {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
            verifier.push(meter, ST::U16)?;
        }
        Bytecode::CastU32 => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            if !operand.is_integer() {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
            verifier.push(meter, ST::U32)?;
        }
        Bytecode::CastU256 => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            if !operand.is_integer() {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
            verifier.push(meter, ST::U256)?;
        }
        Bytecode::PackVariant(_)
        | Bytecode::PackVariantGeneric(_)
        | Bytecode::UnpackVariant(_)
        | Bytecode::UnpackVariantGeneric(_)
        | Bytecode::UnpackVariantImmRef(_)
        | Bytecode::UnpackVariantGenericImmRef(_)
        | Bytecode::UnpackVariantMutRef(_)
        | Bytecode::UnpackVariantGenericMutRef(_)
        | Bytecode::VariantSwitch(_) => {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Unexpected variant opcode in version 0".to_string()),
            );
        }
    };
    Ok(())
}

//
// Helpers functions for types
//

fn materialize_type(struct_handle: DatatypeHandleIndex, type_args: &Signature) -> SignatureToken {
    if type_args.is_empty() {
        ST::Datatype(struct_handle)
    } else {
        ST::DatatypeInstantiation(Box::new((struct_handle, type_args.0.clone())))
    }
}

fn instantiate(token: &SignatureToken, subst: &Signature) -> SignatureToken {
    use SignatureToken::*;

    if subst.0.is_empty() {
        return token.clone();
    }

    match token {
        Bool => Bool,
        U8 => U8,
        U16 => U16,
        U32 => U32,
        U64 => U64,
        U128 => U128,
        U256 => U256,
        Address => Address,
        Signer => Signer,
        Vector(ty) => Vector(Box::new(instantiate(ty, subst))),
        Datatype(idx) => Datatype(*idx),
        DatatypeInstantiation(s) => {
            let (idx, struct_type_args) = &**s;
            DatatypeInstantiation(Box::new((
                *idx,
                struct_type_args
                    .iter()
                    .map(|ty| instantiate(ty, subst))
                    .collect(),
            )))
        }
        Reference(ty) => Reference(Box::new(instantiate(ty, subst))),
        MutableReference(ty) => MutableReference(Box::new(instantiate(ty, subst))),
        TypeParameter(idx) => {
            // Assume that the caller has previously parsed and verified the structure of the
            // file and that this guarantees that type parameter indices are always in bounds.
            debug_assert!((*idx as usize) < subst.len());
            subst.0[*idx as usize].clone()
        }
    }
}

fn get_vector_element_type(
    vector_ref_ty: SignatureToken,
    mut_ref_only: bool,
) -> Option<SignatureToken> {
    use SignatureToken::*;
    match vector_ref_ty {
        Reference(referred_type) => {
            if mut_ref_only {
                None
            } else if let ST::Vector(element_type) = *referred_type {
                Some(*element_type)
            } else {
                None
            }
        }
        MutableReference(referred_type) => {
            if let ST::Vector(element_type) = *referred_type {
                Some(*element_type)
            } else {
                None
            }
        }
        _ => None,
    }
}
