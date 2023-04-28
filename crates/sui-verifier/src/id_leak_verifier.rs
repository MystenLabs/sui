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
use move_binary_format::{
    binary_views::{BinaryIndexedView, FunctionView},
    errors::PartialVMError,
    file_format::{
        Bytecode, CodeOffset, CompiledModule, FunctionDefinitionIndex, FunctionHandle, LocalIndex,
        StructDefinition, StructFieldInformation,
    },
};
use move_bytecode_verifier::{
    absint::{AbstractDomain, AbstractInterpreter, JoinResult, TransferFunctions},
    meter::{Meter, Scope},
};
use move_core_types::{
    account_address::AccountAddress, ident_str, identifier::IdentStr, vm_status::StatusCode,
};
use std::{collections::BTreeMap, error::Error};
use sui_types::{
    clock::CLOCK_MODULE_NAME,
    error::{ExecutionError, VMMVerifierErrorSubStatusCode},
    id::OBJECT_MODULE_NAME,
    sui_system_state::SUI_SYSTEM_MODULE_NAME,
    SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_ADDRESS,
};

use crate::{verification_failure, TEST_SCENARIO_MODULE_NAME};
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
const FRESH_ID_FUNCTIONS: &[FunctionIdent] = &[OBJECT_NEW, OBJECT_NEW_UID_FROM_HASH, TS_NEW_OBJECT];
const FUNCTIONS_TO_SKIP: &[FunctionIdent] = &[SUI_SYSTEM_CREATE, SUI_CLOCK_CREATE];

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
    meter: &mut impl Meter,
) -> Result<(), ExecutionError> {
    verify_id_leak(module, meter)
}

fn verify_id_leak(module: &CompiledModule, meter: &mut impl Meter) -> Result<(), ExecutionError> {
    let binary_view = BinaryIndexedView::Module(module);
    for (index, func_def) in module.function_defs.iter().enumerate() {
        let code = match func_def.code.as_ref() {
            Some(code) => code,
            None => continue,
        };
        let handle = binary_view.function_handle_at(func_def.function);
        let func_view =
            FunctionView::function(module, FunctionDefinitionIndex(index as u16), code, handle);
        let initial_state = AbstractState::new(&func_view);
        let mut verifier = IDLeakAnalysis::new(&binary_view, &func_view);
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
                if let Some(message) = err.source().as_ref() {
                    let function_name = binary_view
                        .identifier_at(binary_view.function_handle_at(func_def.function).name);
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
    pub fn new(function_view: &FunctionView) -> Self {
        let mut state = AbstractState {
            locals: BTreeMap::new(),
        };

        for param_idx in 0..function_view.parameters().len() {
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
        meter: &mut impl Meter,
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
    binary_view: &'a BinaryIndexedView<'a>,
    function_view: &'a FunctionView<'a>,
    stack: Vec<AbstractValue>,
}

impl<'a> IDLeakAnalysis<'a> {
    fn new(binary_view: &'a BinaryIndexedView<'a>, function_view: &'a FunctionView<'a>) -> Self {
        Self {
            binary_view,
            function_view,
            stack: vec![],
        }
    }

    fn stack_popn(&mut self, n: usize) {
        let new_len = self.stack.len() - n;
        self.stack.drain(new_len..);
    }

    fn stack_pushn(&mut self, n: usize, val: AbstractValue) {
        let new_len = self.stack.len() + n;
        self.stack.resize(new_len, val);
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
            .function_def_at(self.function_view.index().unwrap())
            .unwrap();
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
        meter: &mut impl Meter,
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
    verifier.stack_popn(parameters.len());

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
        verifier.stack.push(AbstractValue::Fresh);
    } else {
        verifier.stack_pushn(return_.0.len(), AbstractValue::Other);
    }
    Ok(())
}

fn num_fields(struct_def: &StructDefinition) -> usize {
    match &struct_def.field_information {
        StructFieldInformation::Native => 0,
        StructFieldInformation::Declared(fields) => fields.len(),
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
        .struct_handle_at(struct_def.struct_handle);
    let num_fields = num_fields(struct_def);
    verifier.stack_popn(num_fields - 1);
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
    verifier.stack.push(AbstractValue::Other);
    Ok(())
}

fn unpack(verifier: &mut IDLeakAnalysis, struct_def: &StructDefinition) {
    verifier.stack.pop().unwrap();
    verifier.stack_pushn(num_fields(struct_def), AbstractValue::Other);
}

fn execute_inner(
    verifier: &mut IDLeakAnalysis,
    state: &mut AbstractState,
    bytecode: &Bytecode,
    _: CodeOffset,
    meter: &mut impl Meter,
) -> Result<(), PartialVMError> {
    meter.add(Scope::Function, STEP_BASE_COST)?;
    // TODO: Better diagnostics with location
    match bytecode {
        Bytecode::Pop => {
            verifier.stack.pop().unwrap();
        }
        Bytecode::CopyLoc(_local) => {
            // cannot copy a UID
            verifier.stack.push(AbstractValue::Other);
        }
        Bytecode::MoveLoc(local) => {
            let value = state.locals.remove(local).unwrap();
            verifier.stack.push(value);
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
            verifier.stack.push(AbstractValue::Other);
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
            verifier.stack.push(AbstractValue::Other);
        }
        Bytecode::WriteRef => {
            verifier.stack.pop().unwrap();
            verifier.stack.pop().unwrap();
        }

        // These bytecodes produce references, and hence cannot be ID.
        Bytecode::MutBorrowLoc(_)
        | Bytecode::ImmBorrowLoc(_) => verifier.stack.push(AbstractValue::Other),

        | Bytecode::MutBorrowField(_)
        | Bytecode::MutBorrowFieldGeneric(_)
        | Bytecode::ImmBorrowField(_)
        | Bytecode::ImmBorrowFieldGeneric(_) => {
            verifier.stack.pop().unwrap();
            verifier.stack.push(AbstractValue::Other);
        }

        // These bytecodes are not allowed, and will be
        // flagged as error in a different verifier.
        Bytecode::MoveFrom(_)
                | Bytecode::MoveFromGeneric(_)
                | Bytecode::MoveTo(_)
                | Bytecode::MoveToGeneric(_)
                | Bytecode::ImmBorrowGlobal(_)
                | Bytecode::MutBorrowGlobal(_)
                | Bytecode::ImmBorrowGlobalGeneric(_)
                | Bytecode::MutBorrowGlobalGeneric(_)
                | Bytecode::Exists(_)
                | Bytecode::ExistsGeneric(_) => {
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
            verifier.stack_popn(verifier.function_view.return_().len())
        }

        Bytecode::BrTrue(_) | Bytecode::BrFalse(_) | Bytecode::Abort => {
            verifier.stack.pop().unwrap();
        }

        // These bytecodes produce constants, and hence cannot be ID.
        Bytecode::LdTrue | Bytecode::LdFalse | Bytecode::LdU8(_) | Bytecode::LdU16(_)| Bytecode::LdU32(_)  | Bytecode::LdU64(_) | Bytecode::LdU128(_)| Bytecode::LdU256(_)  | Bytecode::LdConst(_) => {
            verifier.stack.push(AbstractValue::Other);
        }

        Bytecode::Pack(idx) => {
            let struct_def = expect_ok(verifier.binary_view.struct_def_at(*idx))?;
            pack(verifier, struct_def)?;
        }
        Bytecode::PackGeneric(idx) => {
            let struct_inst = expect_ok(verifier.binary_view.struct_instantiation_at(*idx))?;
            let struct_def = expect_ok(verifier.binary_view.struct_def_at(struct_inst.def))?;
            pack(verifier, struct_def)?;
        }
        Bytecode::Unpack(idx) => {
            let struct_def = expect_ok(verifier.binary_view.struct_def_at(*idx))?;
            unpack(verifier, struct_def);
        }
        Bytecode::UnpackGeneric(idx) => {
            let struct_inst = expect_ok(verifier.binary_view.struct_instantiation_at(*idx))?;
            let struct_def = expect_ok(verifier.binary_view.struct_def_at(struct_inst.def))?;
            unpack(verifier, struct_def);
        }

        Bytecode::VecPack(_, num) => {
            verifier.stack_popn(*num as usize);
            verifier.stack.push(AbstractValue::Other);
        }

        Bytecode::VecPushBack(_) => {
            verifier.stack.pop().unwrap();
            verifier.stack.pop().unwrap();
        }

        Bytecode::VecUnpack(_, num) => {
            verifier.stack.pop().unwrap();
            verifier.stack_pushn(*num as usize, AbstractValue::Other);
        }

        Bytecode::VecSwap(_) => {
            verifier.stack.pop().unwrap();
            verifier.stack.pop().unwrap();
            verifier.stack.pop().unwrap();
        }
    };
    Ok(())
}

fn expect_ok<T>(res: Result<T, PartialVMError>) -> Result<T, PartialVMError> {
    match res {
        Ok(x) => Ok(x),
        Err(partial_vm_error) => {
            let msg = format!(
                "Should have been verified to be safe by the Move bytecode verifier, \
            Got error: {partial_vm_error:?}"
            );
            debug_assert!(false, "{msg}");
            // This is an internal error, but we cannot accept the module as safe
            Err(PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(msg))
        }
    }
}
