// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines the transfer functions for verifying type safety of a procedure body.
//! It does not utilize control flow, but does check each block independently

use std::{borrow::Borrow, num::NonZeroU64};

use indexmap::IndexMap;
use move_abstract_interpreter::{absint::FunctionContext, control_flow_graph::ControlFlowGraph};
use move_abstract_stack::AbstractStack;
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        AbilitySet, Bytecode, CodeOffset, DatatypeHandleIndex, EnumDefinition, FieldHandleIndex,
        FunctionDefinitionIndex, FunctionHandle, JumpTableInner, LocalIndex, Signature,
        SignatureToken, SignatureToken as ST, StructDefinition, StructDefinitionIndex,
        StructFieldInformation, VariantDefinition, VariantJumpTable,
    },
    safe_unwrap, safe_unwrap_err, CompiledModule,
};
use move_bytecode_verifier_meter::{Meter, Scope};
use move_core_types::vm_status::StatusCode;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct TypeIndex(usize);

struct Locals {
    param_count: usize,
    parameters: Vec<TypeIndex>,
    locals: Vec<TypeIndex>,
}

const TYPE_PUSH_COST: u128 = 1;

impl Locals {
    fn new(parameters: Vec<TypeIndex>, locals: Vec<TypeIndex>) -> Self {
        Self {
            param_count: parameters.len(),
            parameters,
            locals,
        }
    }

    fn local_at(&self, i: LocalIndex) -> TypeIndex {
        let idx = i as usize;
        if idx < self.param_count {
            self.parameters[idx]
        } else {
            self.locals[idx - self.param_count]
        }
    }
}

struct TypeSafetyChecker<'a> {
    module: &'a CompiledModule,
    function_context: &'a FunctionContext<'a>,
    types: IndexMap<SignatureToken, Option<AbilitySet>>,
    locals: Locals,
    stack: AbstractStack<TypeIndex>,
}

fn load_type_impl<'a>(
    meter: &mut (impl Meter + ?Sized),
    module: &CompiledModule,
    function_context: &FunctionContext,
    types: &mut IndexMap<SignatureToken, Option<AbilitySet>>,
    t: impl Borrow<SignatureToken>,
) -> PartialVMResult<TypeIndex> {
    if let Some(index) = types.get_index_of(t.borrow()) {
        return Ok(TypeIndex(index));
    }
    meter.add_items(
        Scope::Function,
        TYPE_LOAD_COST_PER_NODE,
        t.borrow().preorder_traversal().count(),
    )?;
    let (index, _) = types.insert_full(t.borrow().clone(), None);
    Ok(TypeIndex(index))
}

fn load_types_impl<'a>(
    meter: &mut (impl Meter + ?Sized),
    module: &CompiledModule,
    function_context: &FunctionContext,
    types: &mut IndexMap<SignatureToken, Option<AbilitySet>>,
    ts: impl IntoIterator<Item = &'a SignatureToken>,
) -> PartialVMResult<Vec<TypeIndex>> {
    ts.into_iter()
        .map(|t| load_type_impl(meter, module, function_context, types, t))
        .collect()
}

impl<'a> TypeSafetyChecker<'a> {
    fn new(
        module: &'a CompiledModule,
        function_context: &'a FunctionContext<'a>,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<Self> {
        let mut types = IndexMap::new();
        let parameters = load_types_impl(
            meter,
            module,
            function_context,
            &mut types,
            &function_context.parameters().0,
        )?;
        let locals = load_types_impl(
            meter,
            module,
            function_context,
            &mut types,
            &function_context.locals().0,
        )?;
        let locals = Locals::new(parameters, locals);
        Ok(Self {
            module,
            function_context,
            types,
            locals,
            stack: AbstractStack::new(),
        })
    }

    fn load_type(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        t: impl Borrow<SignatureToken>,
    ) -> PartialVMResult<TypeIndex> {
        load_type_impl(
            meter,
            self.module,
            self.function_context,
            &mut self.types,
            t,
        )
    }

    fn get_type(&self, i: TypeIndex) -> &SignatureToken {
        self.types.get_index(i.0).unwrap().0
    }

    fn local_at(&self, i: LocalIndex) -> TypeIndex {
        self.locals.local_at(i)
    }

    fn abilities(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        t: &SignatureToken,
    ) -> PartialVMResult<AbilitySet> {
        if !self.types.contains_key(t) {
            self.load_type(meter, t)?;
        }
        self.abilities_loaded(meter, TypeIndex(safe_unwrap!(self.types.get_index_of(t))))
    }

    fn abilities_loaded(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        t: TypeIndex,
    ) -> PartialVMResult<AbilitySet> {
        let (sig, abilities_opt) = safe_unwrap!(self.types.get_index(t.0));
        match abilities_opt {
            Some(abilities) => Ok(*abilities),
            None => {
                let abilities = self.compute_abilities(meter, &sig.clone())?;
                safe_unwrap!(self.types.get_index_mut(t.0))
                    .1
                    .insert(abilities);
                Ok(abilities)
            }
        }
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
        ty: impl Borrow<SignatureToken>,
    ) -> PartialVMResult<()> {
        let t = self.load_type(meter, ty)?;
        self.push_loaded(meter, t)
    }

    fn push_n(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        ty: impl Borrow<SignatureToken>,
        n: u64,
    ) -> PartialVMResult<()> {
        let t = self.load_type(meter, ty)?;
        self.push_loaded_n(meter, t, n)
    }

    fn push_loaded(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        t: TypeIndex,
    ) -> PartialVMResult<()> {
        self.charge_push(meter, 1)?;
        safe_unwrap_err!(self.stack.push(t));
        Ok(())
    }

    fn push_loaded_n(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        t: TypeIndex,
        n: u64,
    ) -> PartialVMResult<()> {
        self.charge_push(meter, n)?;
        safe_unwrap_err!(self.stack.push_n(t, n));
        Ok(())
    }

    fn charge_push(&mut self, meter: &mut (impl Meter + ?Sized), n: u64) -> PartialVMResult<()> {
        meter.add(Scope::Function, TYPE_PUSH_COST * (n as u128))?;
        Ok(())
    }

    pub fn compute_abilities(
        &mut self,
        meter: &mut (impl Meter + ?Sized),
        ty: &SignatureToken,
    ) -> PartialVMResult<AbilitySet> {
        use SignatureToken as S;

        Ok(match ty {
            S::Bool | S::U8 | S::U16 | S::U32 | S::U64 | S::U128 | S::U256 | S::Address => {
                AbilitySet::PRIMITIVES
            }

            S::Reference(_) | S::MutableReference(_) => AbilitySet::REFERENCES,
            S::Signer => AbilitySet::SIGNER,
            S::TypeParameter(idx) => self.function_context.type_parameters()[*idx as usize],
            S::Datatype(idx) => {
                let sh = self.module.datatype_handle_at(*idx);
                sh.abilities
            }
            S::Vector(inner) => {
                if !self.types.contains_key(ty) {
                    self.load_type(meter, ty.clone())?;
                }
                if let Some(abilities) = &self.types[ty] {
                    *abilities
                } else {
                    let abilities = AbilitySet::polymorphic_abilities(
                        AbilitySet::VECTOR,
                        vec![false],
                        vec![self.compute_abilities(meter, inner)?],
                    )?;
                    self.types[ty] = Some(abilities);
                    abilities
                }
            }
            S::DatatypeInstantiation(inst) => {
                if !self.types.contains_key(ty) {
                    self.load_type(meter, ty.clone())?;
                }
                let (idx, type_args) = &**inst;
                let sh = self.module.datatype_handle_at(*idx);
                let declared_abilities = sh.abilities;
                let type_arg_abilities = type_args
                    .iter()
                    .map(|arg| self.compute_abilities(meter, arg))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let abilities = AbilitySet::polymorphic_abilities(
                    declared_abilities,
                    sh.type_parameters.iter().map(|param| param.is_phantom),
                    type_arg_abilities,
                )?;
                self.types[ty] = Some(abilities);
                abilities
            }
        })
    }
}

pub(crate) fn verify<'a>(
    module: &'a CompiledModule,
    function_context: &'a FunctionContext<'a>,
    meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    let verifier = &mut TypeSafetyChecker::new(module, function_context, meter)?;

    for block_id in function_context.cfg().blocks() {
        for offset in function_context.cfg().instr_indexes(block_id) {
            let code = &verifier.function_context.code();
            let instr = &code.code[offset as usize];
            let jump_tables = &code.jump_tables;
            verify_instr(verifier, instr, jump_tables, offset, meter)?
        }
    }

    Ok(())
}

// Verifies:
// * Top of stack is an immutable reference
// * The type pointed to by the reference is the same as enum definition expected in as the "head
//   constructor" for the jump table. This is important for exhaustivity.
// * The variant tags in the jump table are both unique, and complete for the specified enum.
fn variant_switch(
    verifier: &mut TypeSafetyChecker,
    offset: CodeOffset,
    jump_table: &VariantJumpTable,
) -> PartialVMResult<()> {
    let operand = safe_unwrap_err!(verifier.stack.pop());

    // Check: type is an immutable reference, and get inner type
    let inner_type = match verifier.get_type(operand) {
        ST::Reference(inner) => inner,
        _ => return Err(verifier.error(StatusCode::ENUM_SWITCH_BAD_OPERAND, offset)),
    };

    // Check: The type of the reference is the same as the enum definition expected in
    // the jump table.
    let handle = match &**inner_type {
        SignatureToken::Datatype(handle) => handle,
        SignatureToken::DatatypeInstantiation(inst) => &inst.0,
        _ => return Err(verifier.error(StatusCode::ENUM_TYPE_MISMATCH, offset)),
    };
    let enum_def = {
        let enum_def = verifier.module.enum_def_at(jump_table.head_enum);
        if handle != &enum_def.enum_handle {
            return Err(verifier.error(StatusCode::ENUM_TYPE_MISMATCH, offset));
        }
        enum_def
    };

    // Cardinality check is sufficient to guarantee exhaustivity.
    let JumpTableInner::Full(jt) = &jump_table.jump_table;
    if jt.len() != enum_def.variants.len() {
        return Err(verifier.error(StatusCode::INVALID_ENUM_SWITCH, offset));
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
    let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
    if mut_ && !operand.is_mutable_reference() {
        return Err(verifier.error(StatusCode::BORROWFIELD_TYPE_MISMATCH_ERROR, offset));
    }

    // check the reference on the stack is the expected type.
    // Load the type that owns the field according to the instruction.
    // For generic fields access, this step materializes that type
    let field_handle = verifier.module.field_handle_at(field_handle_index);
    let struct_def = verifier.module.struct_def_at(field_handle.owner);
    let expected_type = &materialize_type(struct_def.struct_handle, type_args);
    match operand {
        ST::Reference(inner) | ST::MutableReference(inner) if expected_type == &**inner => (),
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
    let loc_signature = verifier.get_type(verifier.local_at(idx)).clone();

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
    let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
    if operand != &ST::Address {
        return Err(verifier.error(StatusCode::BORROWGLOBAL_TYPE_MISMATCH_ERROR, offset));
    }

    let struct_def = verifier.module.struct_def_at(idx);
    let struct_type = materialize_type(struct_def.struct_handle, type_args);
    if !verifier.abilities(meter, &struct_type)?.has_key() {
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
    let parameters = verifier.module.signature_at(function_handle.parameters);
    for parameter in parameters.0.iter().rev() {
        let arg = safe_unwrap_err!(verifier.stack.pop());
        let parameter = verifier.load_type(meter, instantiate(parameter, type_actuals))?;
        if arg != parameter {
            return Err(verifier.error(StatusCode::CALL_TYPE_MISMATCH_ERROR, offset));
        }
    }
    // type actual constraints against formals have already beeh checked in signature checker
    for return_type in &verifier.module.signature_at(function_handle.return_).0 {
        verifier.push(meter, instantiate(return_type, type_actuals))?
    }
    Ok(())
}

fn type_fields_signature(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    struct_def: &StructDefinition,
    type_args: &Signature,
) -> PartialVMResult<Vec<TypeIndex>> {
    match &struct_def.field_information {
        StructFieldInformation::Native => {
            // TODO: this is more of "unreachable"
            Err(verifier.error(StatusCode::PACK_TYPE_MISMATCH_ERROR, offset))
        }
        StructFieldInformation::Declared(fields) => fields
            .iter()
            .map(|field_def| {
                verifier.load_type(meter, instantiate(&field_def.signature.0, type_args))
            })
            .collect(),
    }
}

fn pack_struct(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    struct_def: &StructDefinition,
    type_args: &Signature,
) -> PartialVMResult<()> {
    let field_sig = type_fields_signature(verifier, meter, offset, struct_def, type_args)?;
    for sig in field_sig.iter().copied().rev() {
        let arg = safe_unwrap_err!(verifier.stack.pop());
        if arg != sig {
            return Err(verifier.error(StatusCode::PACK_TYPE_MISMATCH_ERROR, offset));
        }
    }

    verifier.push(meter, materialize_type(struct_def.struct_handle, type_args))?;
    Ok(())
}

fn unpack_struct(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    struct_def: &StructDefinition,
    type_args: &Signature,
) -> PartialVMResult<()> {
    let struct_type =
        verifier.load_type(meter, materialize_type(struct_def.struct_handle, type_args))?;

    // Pop an abstract value from the stack and check if its type is equal to the one
    // declared.
    let arg = safe_unwrap_err!(verifier.stack.pop());
    if arg != struct_type {
        return Err(verifier.error(StatusCode::UNPACK_TYPE_MISMATCH_ERROR, offset));
    }

    let field_sig = type_fields_signature(verifier, meter, offset, struct_def, type_args)?;
    for sig in field_sig {
        verifier.push_loaded(meter, sig)?
    }
    Ok(())
}

fn pack_enum_variant(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    enum_def: &EnumDefinition,
    variant_def: &VariantDefinition,
    type_args: &Signature,
) -> PartialVMResult<()> {
    let field_sig = variant_def
        .fields
        .iter()
        .map(|field_def| instantiate(&field_def.signature.0, type_args));
    for sig in field_sig.rev() {
        let sig = verifier.load_type(meter, sig)?;
        let arg = safe_unwrap_err!(verifier.stack.pop());
        if arg != sig {
            return Err(verifier.error(StatusCode::PACK_TYPE_MISMATCH_ERROR, offset));
        }
    }

    verifier.push(meter, materialize_type(enum_def.enum_handle, type_args))?;
    Ok(())
}

fn unpack_enum_variant_by_value(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    enum_def: &EnumDefinition,
    variant_def: &VariantDefinition,
    type_args: &Signature,
) -> PartialVMResult<()> {
    let enum_type = verifier.load_type(meter, materialize_type(enum_def.enum_handle, type_args))?;

    // Pop an abstract value from the stack and check if its type is equal to the one
    // declared.
    let arg = safe_unwrap_err!(verifier.stack.pop());
    if arg != enum_type {
        return Err(verifier.error(StatusCode::UNPACK_TYPE_MISMATCH_ERROR, offset));
    }

    let field_sig = variant_def
        .fields
        .iter()
        .map(|field_def| instantiate(&field_def.signature.0, type_args));
    for sig in field_sig {
        verifier.push(meter, sig)?
    }
    Ok(())
}

fn unpack_enum_variant_by_ref(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    mut_: bool,
    enum_def: &EnumDefinition,
    variant_def: &VariantDefinition,
    type_args: &Signature,
) -> PartialVMResult<()> {
    // Pop an abstract value from the stack and check if its type is equal to the one
    // declared.
    let arg = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));

    // If unpacking the enum mutably the value must be a mutable reference.
    // If unpacking the enum immutably the value must be an immutable reference.
    let inner = match (arg, mut_) {
        (ST::Reference(inner), false) => inner,
        (ST::MutableReference(inner), true) => inner,
        _ => return Err(verifier.error(StatusCode::UNPACK_TYPE_MISMATCH_ERROR, offset)),
    };

    let enum_type = materialize_type(enum_def.enum_handle, type_args);
    if &**inner != &enum_type {
        return Err(verifier.error(StatusCode::UNPACK_TYPE_MISMATCH_ERROR, offset));
    }

    let field_sig = variant_def
        .fields
        .iter()
        .map(|field_def| instantiate(&field_def.signature.0, type_args));
    for sig in field_sig {
        let mk_sig = if mut_ {
            ST::MutableReference
        } else {
            ST::Reference
        };
        verifier.push(meter, mk_sig(Box::new(sig)))?
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
    if !verifier.abilities(meter, &struct_type)?.has_key() {
        return Err(verifier.error(
            StatusCode::EXISTS_WITHOUT_KEY_ABILITY_OR_BAD_ARGUMENT,
            offset,
        ));
    }

    let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
    if operand != &ST::Address {
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
    if !verifier.abilities(meter, &struct_type)?.has_key() {
        return Err(verifier.error(StatusCode::MOVEFROM_WITHOUT_KEY_ABILITY, offset));
    }

    let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
    if operand != &ST::Address {
        return Err(verifier.error(StatusCode::MOVEFROM_TYPE_MISMATCH_ERROR, offset));
    }

    verifier.push(meter, struct_type)?;
    Ok(())
}

fn move_to(
    verifier: &mut TypeSafetyChecker,
    meter: &mut (impl Meter + ?Sized),
    offset: CodeOffset,
    struct_def: &StructDefinition,
    type_args: &Signature,
) -> PartialVMResult<()> {
    let struct_type = materialize_type(struct_def.struct_handle, type_args);
    if !verifier.abilities(meter, &struct_type)?.has_key() {
        return Err(verifier.error(StatusCode::MOVETO_WITHOUT_KEY_ABILITY, offset));
    }

    let key_struct_operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
    let signer_reference_operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
    if key_struct_operand != &struct_type {
        return Err(verifier.error(StatusCode::MOVETO_TYPE_MISMATCH_ERROR, offset));
    }
    match signer_reference_operand {
        ST::Reference(inner) => match &**inner {
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
    let operand_idx = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
    let operand_vec = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));

    // check index
    if operand_idx != &ST::U64 {
        return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset));
    }

    // check vector and update stack
    let element_type = match get_vector_element_type(operand_vec, mut_ref_only) {
        Some(ty) if ty == declared_element_type => ty,
        _ => return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset)),
    };
    let element_ref_type = if mut_ref_only {
        ST::MutableReference(Box::new(element_type.clone()))
    } else {
        ST::Reference(Box::new(element_type.clone()))
    };
    verifier.push(meter, element_ref_type)?;

    Ok(())
}

fn verify_instr(
    verifier: &mut TypeSafetyChecker,
    bytecode: &Bytecode,
    jump_tables: &[VariantJumpTable],
    offset: CodeOffset,
    meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    match bytecode {
        Bytecode::Pop => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            let abilities = verifier.abilities_loaded(meter, operand);
            if !abilities?.has_drop() {
                return Err(verifier.error(StatusCode::POP_WITHOUT_DROP_ABILITY, offset));
            }
        }

        Bytecode::BrTrue(_) | Bytecode::BrFalse(_) => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            if operand != &ST::Bool {
                return Err(verifier.error(StatusCode::BR_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::StLoc(idx) => {
            let operand = safe_unwrap_err!(verifier.stack.pop());
            if operand != verifier.local_at(*idx) {
                return Err(verifier.error(StatusCode::STLOC_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Abort => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            if operand != &ST::U64 {
                return Err(verifier.error(StatusCode::ABORT_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Ret => {
            let return_ = &verifier.function_context.return_().0;
            for return_type in return_.iter().rev() {
                let return_type = verifier.load_type(meter, return_type)?;
                let operand = safe_unwrap_err!(verifier.stack.pop());
                if operand != return_type {
                    return Err(verifier.error(StatusCode::RET_TYPE_MISMATCH_ERROR, offset));
                }
            }
        }

        Bytecode::Branch(_) | Bytecode::Nop => (),

        Bytecode::FreezeRef => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            match operand {
                ST::MutableReference(inner) => {
                    verifier.push(meter, ST::Reference(inner.clone()))?
                }
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
            let field_inst = verifier.module.field_instantiation_at(*field_inst_index);
            let type_inst = verifier.module.signature_at(field_inst.type_parameters);
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
            let field_inst = verifier.module.field_instantiation_at(*field_inst_index);
            let type_inst = verifier.module.signature_at(field_inst.type_parameters);
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
            let signature = verifier.module.constant_at(*idx).type_.clone();
            verifier.push(meter, signature)?;
        }

        Bytecode::LdTrue | Bytecode::LdFalse => {
            verifier.push(meter, ST::Bool)?;
        }

        Bytecode::CopyLoc(idx) => {
            let local_signature = verifier.local_at(*idx).clone();
            if !verifier
                .abilities_loaded(meter, local_signature)?
                .has_copy()
            {
                return Err(verifier.error(StatusCode::COPYLOC_WITHOUT_COPY_ABILITY, offset));
            }
            verifier.push_loaded(meter, local_signature)?
        }

        Bytecode::MoveLoc(idx) => {
            let local_signature = verifier.local_at(*idx).clone();
            verifier.push_loaded(meter, local_signature)?
        }

        Bytecode::MutBorrowLoc(idx) => borrow_loc(verifier, meter, offset, true, *idx)?,

        Bytecode::ImmBorrowLoc(idx) => borrow_loc(verifier, meter, offset, false, *idx)?,

        Bytecode::Call(idx) => {
            let function_handle = verifier.module.function_handle_at(*idx);
            call(verifier, meter, offset, function_handle, &Signature(vec![]))?
        }

        Bytecode::CallGeneric(idx) => {
            let func_inst = verifier.module.function_instantiation_at(*idx);
            let func_handle = verifier.module.function_handle_at(func_inst.handle);
            let type_args = &verifier.module.signature_at(func_inst.type_parameters);
            call(verifier, meter, offset, func_handle, type_args)?
        }

        Bytecode::Pack(idx) => {
            let struct_definition = verifier.module.struct_def_at(*idx);
            pack_struct(
                verifier,
                meter,
                offset,
                struct_definition,
                &Signature(vec![]),
            )?
        }

        Bytecode::PackGeneric(idx) => {
            let struct_inst = verifier.module.struct_instantiation_at(*idx);
            let struct_def = verifier.module.struct_def_at(struct_inst.def);
            let type_args = verifier.module.signature_at(struct_inst.type_parameters);
            pack_struct(verifier, meter, offset, struct_def, type_args)?
        }

        Bytecode::Unpack(idx) => {
            let struct_definition = verifier.module.struct_def_at(*idx);
            unpack_struct(
                verifier,
                meter,
                offset,
                struct_definition,
                &Signature(vec![]),
            )?
        }

        Bytecode::UnpackGeneric(idx) => {
            let struct_inst = verifier.module.struct_instantiation_at(*idx);
            let struct_def = verifier.module.struct_def_at(struct_inst.def);
            let type_args = verifier.module.signature_at(struct_inst.type_parameters);
            unpack_struct(verifier, meter, offset, struct_def, type_args)?
        }

        Bytecode::ReadRef => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            match operand {
                ST::Reference(inner) | ST::MutableReference(inner) => {
                    if !verifier.abilities(meter, &inner)?.has_copy() {
                        return Err(
                            verifier.error(StatusCode::READREF_WITHOUT_COPY_ABILITY, offset)
                        );
                    }
                    verifier.push(meter, &**inner)?;
                }
                _ => return Err(verifier.error(StatusCode::READREF_TYPE_MISMATCH_ERROR, offset)),
            }
        }

        Bytecode::WriteRef => {
            let ref_operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            let val_operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            let ref_inner_signature = match ref_operand {
                ST::MutableReference(inner) => &**inner,
                _ => {
                    return Err(
                        verifier.error(StatusCode::WRITEREF_NO_MUTABLE_REFERENCE_ERROR, offset)
                    )
                }
            };
            if !verifier.abilities(meter, ref_inner_signature)?.has_drop() {
                return Err(verifier.error(StatusCode::WRITEREF_WITHOUT_DROP_ABILITY, offset));
            }
            if val_operand != ref_inner_signature {
                return Err(verifier.error(StatusCode::WRITEREF_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::CastU8 => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            if !operand.is_integer() {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
            verifier.push(meter, ST::U8)?;
        }
        Bytecode::CastU64 => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            if !operand.is_integer() {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
            verifier.push(meter, ST::U64)?;
        }
        Bytecode::CastU128 => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
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
            if verifier.get_type(operand1).is_integer() && operand1 == operand2 {
                verifier.push_loaded(meter, operand1)?;
            } else {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Shl | Bytecode::Shr => {
            let operand1 = safe_unwrap_err!(verifier.stack.pop());
            let operand2 = safe_unwrap_err!(verifier.stack.pop());
            if verifier.get_type(operand2).is_integer() && verifier.get_type(operand1) == &ST::U8 {
                verifier.push_loaded(meter, operand2)?;
            } else {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Or | Bytecode::And => {
            let operand1 = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            let operand2 = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            if operand1 == &ST::Bool && operand2 == &ST::Bool {
                verifier.push(meter, ST::Bool)?;
            } else {
                return Err(verifier.error(StatusCode::BOOLEAN_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Not => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            if operand == &ST::Bool {
                verifier.push(meter, ST::Bool)?;
            } else {
                return Err(verifier.error(StatusCode::BOOLEAN_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Eq | Bytecode::Neq => {
            let operand1 = safe_unwrap_err!(verifier.stack.pop());
            let operand2 = safe_unwrap_err!(verifier.stack.pop());
            if verifier.abilities_loaded(meter, operand1)?.has_drop() && operand1 == operand2 {
                verifier.push(meter, ST::Bool)?;
            } else {
                return Err(verifier.error(StatusCode::EQUALITY_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::Lt | Bytecode::Gt | Bytecode::Le | Bytecode::Ge => {
            let operand1 = safe_unwrap_err!(verifier.stack.pop());
            let operand2 = safe_unwrap_err!(verifier.stack.pop());
            if verifier.get_type(operand1).is_integer() && operand1 == operand2 {
                verifier.push(meter, ST::Bool)?
            } else {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
        }

        Bytecode::MutBorrowGlobalDeprecated(idx) => {
            borrow_global(verifier, meter, offset, true, *idx, &Signature(vec![]))?
        }

        Bytecode::MutBorrowGlobalGenericDeprecated(idx) => {
            let struct_inst = verifier.module.struct_instantiation_at(*idx);
            let type_inst = verifier.module.signature_at(struct_inst.type_parameters);
            borrow_global(verifier, meter, offset, true, struct_inst.def, type_inst)?
        }

        Bytecode::ImmBorrowGlobalDeprecated(idx) => {
            borrow_global(verifier, meter, offset, false, *idx, &Signature(vec![]))?
        }

        Bytecode::ImmBorrowGlobalGenericDeprecated(idx) => {
            let struct_inst = verifier.module.struct_instantiation_at(*idx);
            let type_inst = verifier.module.signature_at(struct_inst.type_parameters);
            borrow_global(verifier, meter, offset, false, struct_inst.def, type_inst)?
        }

        Bytecode::ExistsDeprecated(idx) => {
            let struct_def = verifier.module.struct_def_at(*idx);
            exists(verifier, meter, offset, struct_def, &Signature(vec![]))?
        }

        Bytecode::ExistsGenericDeprecated(idx) => {
            let struct_inst = verifier.module.struct_instantiation_at(*idx);
            let struct_def = verifier.module.struct_def_at(struct_inst.def);
            let type_args = verifier.module.signature_at(struct_inst.type_parameters);
            exists(verifier, meter, offset, struct_def, type_args)?
        }

        Bytecode::MoveFromDeprecated(idx) => {
            let struct_def = verifier.module.struct_def_at(*idx);
            move_from(verifier, meter, offset, struct_def, &Signature(vec![]))?
        }

        Bytecode::MoveFromGenericDeprecated(idx) => {
            let struct_inst = verifier.module.struct_instantiation_at(*idx);
            let struct_def = verifier.module.struct_def_at(struct_inst.def);
            let type_args = verifier.module.signature_at(struct_inst.type_parameters);
            move_from(verifier, meter, offset, struct_def, type_args)?
        }

        Bytecode::MoveToDeprecated(idx) => {
            let struct_def = verifier.module.struct_def_at(*idx);
            move_to(verifier, meter, offset, struct_def, &Signature(vec![]))?
        }

        Bytecode::MoveToGenericDeprecated(idx) => {
            let struct_inst = verifier.module.struct_instantiation_at(*idx);
            let struct_def = verifier.module.struct_def_at(struct_inst.def);
            let type_args = verifier.module.signature_at(struct_inst.type_parameters);
            move_to(verifier, meter, offset, struct_def, type_args)?
        }

        Bytecode::VecPack(idx, num) => {
            let element_type = &verifier.module.signature_at(*idx).0[0];
            if let Some(num_to_pop) = NonZeroU64::new(*num) {
                let is_mismatched = verifier
                    .stack
                    .pop_eq_n(num_to_pop)
                    .map(|t| element_type != verifier.get_type(t))
                    .unwrap_or(true);
                if is_mismatched {
                    return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset));
                }
            }
            verifier.push(meter, ST::Vector(Box::new(element_type.clone())))?;
        }

        Bytecode::VecLen(idx) => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            let declared_element_type = &verifier.module.signature_at(*idx).0[0];
            match get_vector_element_type(operand, false) {
                Some(derived_element_type) if derived_element_type == declared_element_type => {
                    verifier.push(meter, ST::U64)?;
                }
                _ => return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset)),
            };
        }

        Bytecode::VecImmBorrow(idx) => {
            let declared_element_type = &verifier.module.signature_at(*idx).0[0];
            borrow_vector_element(verifier, meter, declared_element_type, offset, false)?
        }
        Bytecode::VecMutBorrow(idx) => {
            let declared_element_type = &verifier.module.signature_at(*idx).0[0];
            borrow_vector_element(verifier, meter, declared_element_type, offset, true)?
        }

        Bytecode::VecPushBack(idx) => {
            let operand_elem = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            let operand_vec = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            let declared_element_type = &verifier.module.signature_at(*idx).0[0];
            if declared_element_type != operand_elem {
                return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset));
            }
            match get_vector_element_type(operand_vec, true) {
                Some(derived_element_type) if derived_element_type == declared_element_type => {}
                _ => return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset)),
            };
        }

        Bytecode::VecPopBack(idx) => {
            let operand_vec = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            let declared_element_type = &verifier.module.signature_at(*idx).0[0];
            match get_vector_element_type(operand_vec, true) {
                Some(derived_element_type) if derived_element_type == declared_element_type => {
                    verifier.push(meter, derived_element_type)?;
                }
                _ => return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset)),
            };
        }

        Bytecode::VecUnpack(idx, num) => {
            let operand_vec = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            let declared_element_type = &verifier.module.signature_at(*idx).0[0];
            if operand_vec != &ST::Vector(Box::new(declared_element_type.clone())) {
                return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset));
            }
            verifier.push_n(meter, declared_element_type.clone(), *num)?;
        }

        Bytecode::VecSwap(idx) => {
            let operand_idx2 = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            let operand_idx1 = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            let operand_vec = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            if operand_idx1 != &ST::U64 || operand_idx2 != &ST::U64 {
                return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset));
            }
            let declared_element_type = &verifier.module.signature_at(*idx).0[0];
            match get_vector_element_type(operand_vec, true) {
                Some(derived_element_type) if derived_element_type == declared_element_type => {}
                _ => return Err(verifier.error(StatusCode::TYPE_MISMATCH, offset)),
            };
        }
        Bytecode::CastU16 => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            if !operand.is_integer() {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
            verifier.push(meter, ST::U16)?;
        }
        Bytecode::CastU32 => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            if !operand.is_integer() {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
            verifier.push(meter, ST::U32)?;
        }
        Bytecode::CastU256 => {
            let operand = verifier.get_type(safe_unwrap_err!(verifier.stack.pop()));
            if !operand.is_integer() {
                return Err(verifier.error(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR, offset));
            }
            verifier.push(meter, ST::U256)?;
        }
        Bytecode::PackVariant(vidx) => {
            let handle = verifier.module.variant_handle_at(*vidx);
            let enum_def = verifier.module.enum_def_at(handle.enum_def);
            let variant_def = &enum_def.variants[handle.variant as usize];
            pack_enum_variant(
                verifier,
                meter,
                offset,
                enum_def,
                variant_def,
                &Signature(vec![]),
            )?
        }
        Bytecode::PackVariantGeneric(vidx) => {
            let handle = verifier.module.variant_instantiation_handle_at(*vidx);
            let enum_inst = verifier.module.enum_instantiation_at(handle.enum_def);
            let type_args = verifier.module.signature_at(enum_inst.type_parameters);
            let enum_def = verifier.module.enum_def_at(enum_inst.def);
            let variant_def = &enum_def.variants[handle.variant as usize];
            pack_enum_variant(verifier, meter, offset, enum_def, variant_def, type_args)?
        }
        Bytecode::UnpackVariant(vidx) => {
            let handle = verifier.module.variant_handle_at(*vidx);
            let enum_def = verifier.module.enum_def_at(handle.enum_def);
            let variant_def = &enum_def.variants[handle.variant as usize];
            unpack_enum_variant_by_value(
                verifier,
                meter,
                offset,
                enum_def,
                variant_def,
                &Signature(vec![]),
            )?
        }
        Bytecode::UnpackVariantGeneric(vidx) => {
            let handle = verifier.module.variant_instantiation_handle_at(*vidx);
            let enum_inst = verifier.module.enum_instantiation_at(handle.enum_def);
            let type_args = verifier.module.signature_at(enum_inst.type_parameters);
            let enum_def = verifier.module.enum_def_at(enum_inst.def);
            let variant_def = &enum_def.variants[handle.variant as usize];
            unpack_enum_variant_by_value(verifier, meter, offset, enum_def, variant_def, type_args)?
        }
        Bytecode::UnpackVariantImmRef(vidx) => {
            let handle = verifier.module.variant_handle_at(*vidx);
            let enum_def = verifier.module.enum_def_at(handle.enum_def);
            let variant_def = &enum_def.variants[handle.variant as usize];
            unpack_enum_variant_by_ref(
                verifier,
                meter,
                offset,
                /* mut_ */ false,
                enum_def,
                variant_def,
                &Signature(vec![]),
            )?
        }
        Bytecode::UnpackVariantMutRef(vidx) => {
            let handle = verifier.module.variant_handle_at(*vidx);
            let enum_def = verifier.module.enum_def_at(handle.enum_def);
            let variant_def = &enum_def.variants[handle.variant as usize];
            unpack_enum_variant_by_ref(
                verifier,
                meter,
                offset,
                /* mut_ */ true,
                enum_def,
                variant_def,
                &Signature(vec![]),
            )?
        }
        Bytecode::UnpackVariantGenericImmRef(vidx) => {
            let handle = verifier.module.variant_instantiation_handle_at(*vidx);
            let enum_inst = verifier.module.enum_instantiation_at(handle.enum_def);
            let type_args = verifier.module.signature_at(enum_inst.type_parameters);
            let enum_def = verifier.module.enum_def_at(enum_inst.def);
            let variant_def = &enum_def.variants[handle.variant as usize];
            unpack_enum_variant_by_ref(
                verifier,
                meter,
                offset,
                /* mut_ */ false,
                enum_def,
                variant_def,
                type_args,
            )?
        }
        Bytecode::UnpackVariantGenericMutRef(vidx) => {
            let handle = verifier.module.variant_instantiation_handle_at(*vidx);
            let enum_inst = verifier.module.enum_instantiation_at(handle.enum_def);
            let type_args = verifier.module.signature_at(enum_inst.type_parameters);
            let enum_def = verifier.module.enum_def_at(enum_inst.def);
            let variant_def = &enum_def.variants[handle.variant as usize];
            unpack_enum_variant_by_ref(
                verifier,
                meter,
                offset,
                /* mut_ */ true,
                enum_def,
                variant_def,
                type_args,
            )?
        }
        Bytecode::VariantSwitch(jti) => {
            let jt = &jump_tables[jti.0 as usize];
            variant_switch(verifier, offset, jt)?
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
        DatatypeInstantiation(inst) => {
            let (idx, type_args) = &**inst;
            DatatypeInstantiation(Box::new((
                *idx,
                type_args.iter().map(|ty| instantiate(ty, subst)).collect(),
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
    vector_ref_ty: &SignatureToken,
    mut_ref_only: bool,
) -> Option<&SignatureToken> {
    use SignatureToken::*;
    match vector_ref_ty {
        Reference(referred_type) => {
            if mut_ref_only {
                None
            } else if let ST::Vector(element_type) = &**referred_type {
                Some(&**element_type)
            } else {
                None
            }
        }
        MutableReference(referred_type) => {
            if let ST::Vector(element_type) = &**referred_type {
                Some(&**element_type)
            } else {
                None
            }
        }
        _ => None,
    }
}
