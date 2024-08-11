// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Objects whose struct type has key ability represent Sui objects.
//! They have unique IDs that should never be reused. This verifier makes
//! sure that the id field of Sui objects never get leaked.
//! Unpack is the only bytecode that could extract the id field out of
//! a Sui object. From there, we track the flow of the value and make
//! sure it can never get leaked outside of the function. There are four
//! ways it can happen:
//! 1. Returned
//! 2. Written into a mutable reference
//! 3. Added to a vector
//! 4. Passed to a function cal::;
use move_abstract_interpreter::absint::{
    AbstractDomain, AbstractInterpreter, FunctionContext, JoinResult, TransferFunctions,
};
use move_abstract_stack::AbstractStack;
use move_binary_format::{
    errors::PartialVMError,
    file_format::{
        Bytecode, CodeOffset, CompiledModule, FunctionDefinitionIndex, FunctionHandle, LocalIndex,
        StructDefinition, StructFieldInformation,
    },
};
use move_bytecode_verifier_meter::{Meter, Scope};
use move_core_types::{
    account_address::AccountAddress, ident_str, identifier::IdentStr, vm_status::StatusCode,
};
use std::{collections::BTreeMap, error::Error, num::NonZeroU64};
use sui_types::bridge::BRIDGE_MODULE_NAME;
use sui_types::deny_list_v1::{DENY_LIST_CREATE_FUNC, DENY_LIST_MODULE};
use sui_types::{
    authenticator_state::AUTHENTICATOR_STATE_MODULE_NAME,
    clock::CLOCK_MODULE_NAME,
    error::{ExecutionError, VMMVerifierErrorSubStatusCode},
    id::OBJECT_MODULE_NAME,
    randomness_state::RANDOMNESS_MODULE_NAME,
    sui_system_state::SUI_SYSTEM_MODULE_NAME,
    BRIDGE_ADDRESS, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_ADDRESS,
};

use crate::{
    check_for_verifier_timeout, to_verification_timeout_error, verification_failure,
    TEST_SCENARIO_MODULE_NAME,
};
pub(crate) const JOIN_BASE_COST: u128 = 10;
pub(crate) const JOIN_PER_LOCAL_COST: u128 = 5;
pub(crate) const STEP_BASE_COST: u128 = 15;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AbstractValue {
    Fresh,
    Other,
}

type FunctionIdent<'a> = (&'a AccountAddress, &'a IdentStr, &'a IdentStr);
const OBJECT_NEW: FunctionIdent = (
    &SUI_FRAMEWORK_ADDRESS,
    OBJECT_MODULE_NAME,
    ident_str!("new"),
);
const OBJECT_NEW_UID_FROM_HASH: FunctionIdent = (
    &SUI_FRAMEWORK_ADDRESS,
    OBJECT_MODULE_NAME,
    ident_str!("new_uid_from_hash"),
);
const TS_NEW_OBJECT: FunctionIdent = (
    &SUI_FRAMEWORK_ADDRESS,
    ident_str!(TEST_SCENARIO_MODULE_NAME),
    ident_str!("new_object"),
);
const SUI_SYSTEM_CREATE: FunctionIdent = (
    &SUI_SYSTEM_ADDRESS,
    SUI_SYSTEM_MODULE_NAME,
    ident_str!("create"),
);
const SUI_CLOCK_CREATE: FunctionIdent = (
    &SUI_FRAMEWORK_ADDRESS,
    CLOCK_MODULE_NAME,
    ident_str!("create"),
);
const SUI_AUTHENTICATOR_STATE_CREATE: FunctionIdent = (
    &SUI_FRAMEWORK_ADDRESS,
    AUTHENTICATOR_STATE_MODULE_NAME,
    ident_str!("create"),
);
const SUI_RANDOMNESS_STATE_CREATE: FunctionIdent = (
    &SUI_FRAMEWORK_ADDRESS,
    RANDOMNESS_MODULE_NAME,
    ident_str!("create"),
);
const SUI_DENY_LIST_CREATE: FunctionIdent = (
    &SUI_FRAMEWORK_ADDRESS,
    DENY_LIST_MODULE,
    DENY_LIST_CREATE_FUNC,
);

const SUI_BRIDGE_CREATE: FunctionIdent =
    (&BRIDGE_ADDRESS, BRIDGE_MODULE_NAME, ident_str!("create"));
const FRESH_ID_FUNCTIONS: &[FunctionIdent] = &[OBJECT_NEW, OBJECT_NEW_UID_FROM_HASH, TS_NEW_OBJECT];
const FUNCTIONS_TO_SKIP: &[FunctionIdent] = &[
    SUI_SYSTEM_CREATE,
    SUI_CLOCK_CREATE,
    SUI_AUTHENTICATOR_STATE_CREATE,
    SUI_RANDOMNESS_STATE_CREATE,
    SUI_DENY_LIST_CREATE,
    SUI_BRIDGE_CREATE,
];

impl AbstractValue {
    pub fn join(&self, value: &AbstractValue) -> AbstractValue {
        if self == value {
            *value
        } else {
            AbstractValue::Other
        }
    }
}

pub fn verify_module(
    module: &CompiledModule,
    meter: &mut (impl Meter + ?Sized),
) -> Result<(), ExecutionError> {
    verify_id_leak(module, meter)
}

fn verify_id_leak(
    module: &CompiledModule,
    meter: &mut (impl Meter + ?Sized),
) -> Result<(), ExecutionError> {
    for (index, func_def) in module.function_defs.iter().enumerate() {
        let code = match func_def.code.as_ref() {
            Some(code) => code,
            None => continue,
        };
        let handle = module.function_handle_at(func_def.function);
        let func_view =
            FunctionContext::new(module, FunctionDefinitionIndex(index as u16), code, handle);
        let initial_state = AbstractState::new(&func_view);
        let mut verifier = IDLeakAnalysis::new(module, &func_view);
        let function_to_verify = verifier.cur_function();
        if FUNCTIONS_TO_SKIP
            .iter()
            .any(|to_skip| function_to_verify == *to_skip)
        {
            continue;
        }
        verifier
            .analyze_function(initial_state, &func_view, meter)
            .map_err(|err| {
                // Handle verifificaiton timeout specially
                if check_for_verifier_timeout(&err.major_status()) {
                    to_verification_timeout_error(err.to_string())
                } else if let Some(message) = err.source().as_ref() {
                    let function_name =
                        module.identifier_at(module.function_handle_at(func_def.function).name);
                    let module_name = module.self_id();
                    verification_failure(format!(
                        "{} Found in {module_name}::{function_name}",
                        message
                    ))
                } else {
                    verification_failure(err.to_string())
                }
            })?;
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AbstractState {
    locals: BTreeMap<LocalIndex, AbstractValue>,
}

impl AbstractState {
    /// create a new abstract state
    pub fn new(function_context: &FunctionContext) -> Self {
        let mut state = AbstractState {
            locals: BTreeMap::new(),
        };

        for param_idx in 0..function_context.parameters().len() {
            state
                .locals
                .insert(param_idx as LocalIndex, AbstractValue::Other);
        }

        state
    }
}

impl AbstractDomain for AbstractState {
    /// attempts to join state to self and returns the result
    fn join(
        &mut self,
        state: &AbstractState,
        meter: &mut (impl Meter + ?Sized),
    ) -> Result<JoinResult, PartialVMError> {
        meter.add(Scope::Function, JOIN_BASE_COST)?;
        meter.add_items(Scope::Function, JOIN_PER_LOCAL_COST, state.locals.len())?;
        let mut changed = false;
        for (local, value) in &state.locals {
            let old_value = *self.locals.get(local).unwrap_or(&AbstractValue::Other);
            let new_value = value.join(&old_value);
            changed |= new_value != old_value;
            self.locals.insert(*local, new_value);
        }
        if changed {
            Ok(JoinResult::Changed)
        } else {
            Ok(JoinResult::Unchanged)
        }
    }
}

struct IDLeakAnalysis<'a> {
    binary_view: &'a CompiledModule,
    function_context: &'a FunctionContext<'a>,
    stack: AbstractStack<AbstractValue>,
}

impl<'a> IDLeakAnalysis<'a> {
    fn new(binary_view: &'a CompiledModule, function_context: &'a FunctionContext<'a>) -> Self {
        Self {
            binary_view,
            function_context,
            stack: AbstractStack::new(),
        }
    }

    fn stack_popn(&mut self, n: u64) -> Result<(), PartialVMError> {
        let Some(n) = NonZeroU64::new(n) else {
            return Ok(());
        };
        self.stack.pop_any_n(n).map_err(|e| {
            PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION)
                .with_message(format!("Unexpected stack error on pop_n: {e}"))
        })
    }

    fn stack_push(&mut self, val: AbstractValue) -> Result<(), PartialVMError> {
        self.stack.push(val).map_err(|e| {
            PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION)
                .with_message(format!("Unexpected stack error on push: {e}"))
        })
    }

    fn stack_pushn(&mut self, n: u64, val: AbstractValue) -> Result<(), PartialVMError> {
        self.stack.push_n(val, n).map_err(|e| {
            PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION)
                .with_message(format!("Unexpected stack error on push_n: {e}"))
        })
    }

    fn resolve_function(&self, function_handle: &FunctionHandle) -> FunctionIdent<'a> {
        let m = self.binary_view.module_handle_at(function_handle.module);
        let address = self.binary_view.address_identifier_at(m.address);
        let module = self.binary_view.identifier_at(m.name);
        let function = self.binary_view.identifier_at(function_handle.name);
        (address, module, function)
    }

    fn cur_function(&self) -> FunctionIdent<'a> {
        let fdef = self
            .binary_view
            .function_def_at(self.function_context.index().unwrap());
        let handle = self.binary_view.function_handle_at(fdef.function);
        self.resolve_function(handle)
    }
}

impl<'a> TransferFunctions for IDLeakAnalysis<'a> {
    type Error = ExecutionError;
    type State = AbstractState;

    fn execute(
        &mut self,
        state: &mut Self::State,
        bytecode: &Bytecode,
        index: CodeOffset,
        last_index: CodeOffset,
        meter: &mut (impl Meter + ?Sized),
    ) -> Result<(), PartialVMError> {
        execute_inner(self, state, bytecode, index, meter)?;
        // invariant: the stack should be empty at the end of the block
        // If it is not, something is wrong with the implementation, so throw an invariant
        // violation
        if index == last_index && !self.stack.is_empty() {
            let msg = "Invalid stack transitions. Non-zero stack size at the end of the block"
                .to_string();
            debug_assert!(false, "{msg}",);
            return Err(
                PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(msg),
            );
        }
        Ok(())
    }
}

impl<'a> AbstractInterpreter for IDLeakAnalysis<'a> {}

fn call(
    verifier: &mut IDLeakAnalysis,
    function_handle: &FunctionHandle,
) -> Result<(), PartialVMError> {
    let parameters = verifier
        .binary_view
        .signature_at(function_handle.parameters);
    verifier.stack_popn(parameters.len() as u64)?;

    let return_ = verifier.binary_view.signature_at(function_handle.return_);
    let function = verifier.resolve_function(function_handle);
    if FRESH_ID_FUNCTIONS
        .iter()
        .any(|makes_fresh| function == *makes_fresh)
    {
        if return_.0.len() != 1 {
            debug_assert!(false, "{:?} should have a single return value", function);
            return Err(PartialVMError::new(StatusCode::UNKNOWN_VERIFICATION_ERROR)
                .with_message("Should have a single return value".to_string())
                .with_sub_status(
                    VMMVerifierErrorSubStatusCode::MULTIPLE_RETURN_VALUES_NOT_ALLOWED as u64,
                ));
        }
        verifier.stack_push(AbstractValue::Fresh)?;
    } else {
        verifier.stack_pushn(return_.0.len() as u64, AbstractValue::Other)?;
    }
    Ok(())
}

fn num_fields(struct_def: &StructDefinition) -> u64 {
    match &struct_def.field_information {
        StructFieldInformation::Native => 0,
        StructFieldInformation::Declared(fields) => fields.len() as u64,
    }
}

fn pack(
    verifier: &mut IDLeakAnalysis,
    struct_def: &StructDefinition,
) -> Result<(), PartialVMError> {
    // When packing, an object whose struct type has key ability must have the first field as
    // "id". That fields must come from one of the functions that creates a new UID.
    let handle = verifier
        .binary_view
        .datatype_handle_at(struct_def.struct_handle);
    let num_fields = num_fields(struct_def);
    verifier.stack_popn(num_fields - 1)?;
    let last_value = verifier.stack.pop().unwrap();
    if handle.abilities.has_key() && last_value != AbstractValue::Fresh {
        let (cur_package, cur_module, cur_function) = verifier.cur_function();
        let msg = format!(
            "Invalid object creation in {cur_package}::{cur_module}::{cur_function}. \
                Object created without a newly created UID. \
                The UID must come directly from sui::{}::{}. \
                Or for tests, it can come from sui::{}::{}",
            OBJECT_NEW.1, OBJECT_NEW.2, TS_NEW_OBJECT.1, TS_NEW_OBJECT.2,
        );

        return Err(PartialVMError::new(StatusCode::UNKNOWN_VERIFICATION_ERROR)
            .with_message(msg)
            .with_sub_status(VMMVerifierErrorSubStatusCode::INVALID_OBJECT_CREATION as u64));
    }
    verifier.stack_push(AbstractValue::Other)?;
    Ok(())
}

fn unpack(
    verifier: &mut IDLeakAnalysis,
    struct_def: &StructDefinition,
) -> Result<(), PartialVMError> {
    verifier.stack.pop().unwrap();
    verifier.stack_pushn(num_fields(struct_def), AbstractValue::Other)
}

fn execute_inner(
    verifier: &mut IDLeakAnalysis,
    state: &mut AbstractState,
    bytecode: &Bytecode,
    _: CodeOffset,
    meter: &mut (impl Meter + ?Sized),
) -> Result<(), PartialVMError> {
    meter.add(Scope::Function, STEP_BASE_COST)?;
    // TODO: Better diagnostics with location
    match bytecode {
        Bytecode::Pop => {
            verifier.stack.pop().unwrap();
        }
        Bytecode::CopyLoc(_local) => {
            // cannot copy a UID
            verifier.stack_push(AbstractValue::Other)?;
        }
        Bytecode::MoveLoc(local) => {
            let value = state.locals.remove(local).unwrap();
            verifier.stack_push(value)?;
        }
        Bytecode::StLoc(local) => {
            let value = verifier.stack.pop().unwrap();
            state.locals.insert(*local, value);
        }

        // Reference won't be ID.
        Bytecode::FreezeRef
        // ID doesn't have copy ability, hence ReadRef won't produce an ID.
        | Bytecode::ReadRef
        // Following are unary operators that don't apply to ID.
        | Bytecode::CastU8
        | Bytecode::CastU16
        | Bytecode::CastU32
        | Bytecode::CastU64
        | Bytecode::CastU128
        | Bytecode::CastU256
        | Bytecode::Not
        | Bytecode::VecLen(_)
        | Bytecode::VecPopBack(_) => {
            verifier.stack.pop().unwrap();
            verifier.stack_push(AbstractValue::Other)?;
        }

        // These bytecodes don't operate on any value.
        Bytecode::Branch(_)
        | Bytecode::Nop => {}

        // These binary operators cannot produce ID as result.
        Bytecode::Eq
        | Bytecode::Neq
        | Bytecode::Add
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
        | Bytecode::Ge
        | Bytecode::VecImmBorrow(_)
        | Bytecode::VecMutBorrow(_) => {
            verifier.stack.pop().unwrap();
            verifier.stack.pop().unwrap();
            verifier.stack_push(AbstractValue::Other)?;
        }
        Bytecode::WriteRef => {
            verifier.stack.pop().unwrap();
            verifier.stack.pop().unwrap();
        }

        // These bytecodes produce references, and hence cannot be ID.
        Bytecode::MutBorrowLoc(_)
        | Bytecode::ImmBorrowLoc(_) => verifier.stack_push(AbstractValue::Other)?,

        | Bytecode::MutBorrowField(_)
        | Bytecode::MutBorrowFieldGeneric(_)
        | Bytecode::ImmBorrowField(_)
        | Bytecode::ImmBorrowFieldGeneric(_) => {
            verifier.stack.pop().unwrap();
            verifier.stack_push(AbstractValue::Other)?;
        }

        // These bytecodes are not allowed, and will be
        // flagged as error in a different verifier.
        Bytecode::MoveFromDeprecated(_)
                | Bytecode::MoveFromGenericDeprecated(_)
                | Bytecode::MoveToDeprecated(_)
                | Bytecode::MoveToGenericDeprecated(_)
                | Bytecode::ImmBorrowGlobalDeprecated(_)
                | Bytecode::MutBorrowGlobalDeprecated(_)
                | Bytecode::ImmBorrowGlobalGenericDeprecated(_)
                | Bytecode::MutBorrowGlobalGenericDeprecated(_)
                | Bytecode::ExistsDeprecated(_)
                | Bytecode::ExistsGenericDeprecated(_) => {
            panic!("Should have been checked by global_storage_access_verifier.");
        }

        Bytecode::Call(idx) => {
            let function_handle = verifier.binary_view.function_handle_at(*idx);
            call(verifier, function_handle)?;
        }
        Bytecode::CallGeneric(idx) => {
            let func_inst = verifier.binary_view.function_instantiation_at(*idx);
            let function_handle = verifier.binary_view.function_handle_at(func_inst.handle);
            call(verifier, function_handle)?;
        }

        Bytecode::Ret => {
            verifier.stack_popn(verifier.function_context.return_().len() as u64)?
        }

        Bytecode::BrTrue(_) | Bytecode::BrFalse(_) | Bytecode::Abort => {
            verifier.stack.pop().unwrap();
        }

        // These bytecodes produce constants, and hence cannot be ID.
        Bytecode::LdTrue | Bytecode::LdFalse | Bytecode::LdU8(_) | Bytecode::LdU16(_)| Bytecode::LdU32(_)  | Bytecode::LdU64(_) | Bytecode::LdU128(_)| Bytecode::LdU256(_)  | Bytecode::LdConst(_) => {
            verifier.stack_push(AbstractValue::Other)?;
        }

        Bytecode::Pack(idx) => {
            let struct_def = verifier.binary_view.struct_def_at(*idx);
            pack(verifier, struct_def)?;
        }
        Bytecode::PackGeneric(idx) => {
            let struct_inst = verifier.binary_view.struct_instantiation_at(*idx);
            let struct_def = verifier.binary_view.struct_def_at(struct_inst.def);
            pack(verifier, struct_def)?;
        }
        Bytecode::Unpack(idx) => {
            let struct_def = verifier.binary_view.struct_def_at(*idx);
            unpack(verifier, struct_def)?;
        }
        Bytecode::UnpackGeneric(idx) => {
            let struct_inst = verifier.binary_view.struct_instantiation_at(*idx);
            let struct_def = verifier.binary_view.struct_def_at(struct_inst.def);
            unpack(verifier, struct_def)?;
        }

        Bytecode::VecPack(_, num) => {
            verifier.stack_popn(*num )?;
            verifier.stack_push(AbstractValue::Other)?;
        }

        Bytecode::VecPushBack(_) => {
            verifier.stack.pop().unwrap();
            verifier.stack.pop().unwrap();
        }

        Bytecode::VecUnpack(_, num) => {
            verifier.stack.pop().unwrap();
            verifier.stack_pushn(*num, AbstractValue::Other)?;
        }

        Bytecode::VecSwap(_) => {
            verifier.stack.pop().unwrap();
            verifier.stack.pop().unwrap();
            verifier.stack.pop().unwrap();
        }
        Bytecode::PackVariant(vidx) =>  {
            let handle = verifier.binary_view.variant_handle_at(*vidx);
            let variant = verifier.binary_view.variant_def_at(handle.enum_def, handle.variant);
            let num_fields = variant.fields.len();
            verifier.stack_popn(num_fields as u64)?;
            verifier.stack_push(AbstractValue::Other)?;
        }
        Bytecode::PackVariantGeneric(vidx) =>  {
            let handle = verifier.binary_view.variant_instantiation_handle_at(*vidx);
            let enum_inst = verifier.binary_view.enum_instantiation_at(handle.enum_def);
            let variant = verifier.binary_view.variant_def_at(enum_inst.def, handle.variant);
            let num_fields = variant.fields.len();
            verifier.stack_popn(num_fields as u64)?;
            verifier.stack_push(AbstractValue::Other)?;
        }
        Bytecode::UnpackVariant(vidx)
        | Bytecode::UnpackVariantImmRef(vidx)
        | Bytecode::UnpackVariantMutRef(vidx) =>  {
            let handle = verifier.binary_view.variant_handle_at(*vidx);
            let variant = verifier.binary_view.variant_def_at(handle.enum_def, handle.variant);
            let num_fields = variant.fields.len();
            verifier.stack.pop().unwrap();
            verifier.stack_pushn(num_fields as u64, AbstractValue::Other)?;
        }
        Bytecode::UnpackVariantGeneric(vidx)
        | Bytecode::UnpackVariantGenericImmRef(vidx)
        | Bytecode::UnpackVariantGenericMutRef(vidx) =>  {
            let handle = verifier.binary_view.variant_instantiation_handle_at(*vidx);
            let enum_inst = verifier.binary_view.enum_instantiation_at(handle.enum_def);
            let variant = verifier.binary_view.variant_def_at(enum_inst.def, handle.variant);
            let num_fields = variant.fields.len();
            verifier.stack.pop().unwrap();
            verifier.stack_pushn(num_fields as u64, AbstractValue::Other)?;
        }
        Bytecode::VariantSwitch(_) =>  {
            verifier.stack.pop().unwrap();
        }
    };
    Ok(())
}
