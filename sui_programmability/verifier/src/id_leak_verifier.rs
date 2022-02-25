// Copyright (c) 2022, Mysten Labs, Inc.
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
//! 4. Passed to a function call

use crate::verification_failure;
use move_binary_format::{
    binary_views::{BinaryIndexedView, FunctionView},
    file_format::{
        Bytecode, CodeOffset, CompiledModule, FunctionDefinitionIndex, FunctionHandle, LocalIndex,
        StructDefinition, StructFieldInformation,
    },
};
use move_bytecode_verifier::absint::{
    AbstractDomain, AbstractInterpreter, BlockInvariant, BlockPostcondition, JoinResult,
    TransferFunctions,
};
use std::collections::BTreeMap;
use sui_types::{
    error::{SuiError, SuiResult},
    SUI_FRAMEWORK_ADDRESS,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AbstractValue {
    ID,
    NonID,
}

impl AbstractValue {
    pub fn join(&self, value: &AbstractValue) -> AbstractValue {
        if self == value {
            *value
        } else {
            AbstractValue::ID
        }
    }
}

pub fn verify_module(module: &CompiledModule) -> SuiResult {
    verify_id_leak(module)
}

fn verify_id_leak(module: &CompiledModule) -> SuiResult {
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
        let inv_map = verifier.analyze_function(initial_state, &func_view);
        // Report all the join failures
        for (_block_id, BlockInvariant { post, .. }) in inv_map {
            match post {
                BlockPostcondition::Error(err) => match err {
                    SuiError::ModuleVerificationFailure { error } => {
                        return Err(SuiError::ModuleVerificationFailure {
                            error: format!(
                                "ID leak detected in function {}: {}",
                                binary_view.identifier_at(handle.name),
                                error
                            ),
                        });
                    }
                    _ => {
                        panic!("Unexpected error type");
                    }
                },
                // Block might be unprocessed if all predecessors had an error
                BlockPostcondition::Unprocessed | BlockPostcondition::Success => (),
            }
        }
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
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
                .insert(param_idx as LocalIndex, AbstractValue::NonID);
        }

        state
    }
}

impl AbstractDomain for AbstractState {
    /// attempts to join state to self and returns the result
    fn join(&mut self, state: &AbstractState) -> JoinResult {
        let mut changed = false;
        for (local, value) in &state.locals {
            let old_value = *self.locals.get(local).unwrap_or(&AbstractValue::NonID);
            changed |= *value != old_value;
            self.locals.insert(*local, value.join(&old_value));
        }
        if changed {
            JoinResult::Changed
        } else {
            JoinResult::Unchanged
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
}

impl<'a> TransferFunctions for IDLeakAnalysis<'a> {
    type State = AbstractState;
    type AnalysisError = SuiError;

    fn execute(
        &mut self,
        state: &mut Self::State,
        bytecode: &Bytecode,
        index: CodeOffset,
        _: CodeOffset,
    ) -> Result<(), Self::AnalysisError> {
        execute_inner(self, state, bytecode, index)?;
        Ok(())
    }
}

impl<'a> AbstractInterpreter for IDLeakAnalysis<'a> {}

/// Sui::ID::delete function is allowed to take an ID by value.
fn is_call_safe_to_leak(verifier: &IDLeakAnalysis, function_handle: &FunctionHandle) -> bool {
    let m = verifier
        .binary_view
        .module_handle_at(function_handle.module);
    verifier.binary_view.address_identifier_at(m.address) == &SUI_FRAMEWORK_ADDRESS
        && verifier.binary_view.identifier_at(m.name).as_str() == "ID"
        && verifier
            .binary_view
            .identifier_at(function_handle.name)
            .as_str()
            == "delete"
}

fn call(verifier: &mut IDLeakAnalysis, function_handle: &FunctionHandle) -> SuiResult {
    let guaranteed_safe = is_call_safe_to_leak(verifier, function_handle);
    let parameters = verifier
        .binary_view
        .signature_at(function_handle.parameters);
    for _ in 0..parameters.len() {
        if verifier.stack.pop().unwrap() == AbstractValue::ID && !guaranteed_safe {
            return Err(verification_failure(
                "ID leaked through function call.".to_string(),
            ));
        }
    }

    let return_ = verifier.binary_view.signature_at(function_handle.return_);
    for _ in 0..return_.0.len() {
        verifier.stack.push(AbstractValue::NonID);
    }
    Ok(())
}

fn num_fields(struct_def: &StructDefinition) -> usize {
    match &struct_def.field_information {
        StructFieldInformation::Native => 0,
        StructFieldInformation::Declared(fields) => fields.len(),
    }
}

fn pack(verifier: &mut IDLeakAnalysis, struct_def: &StructDefinition) {
    let mut has_id = false;
    for _ in 0..num_fields(struct_def) {
        has_id |= verifier.stack.pop().unwrap() == AbstractValue::ID;
    }
    verifier.stack.push(if has_id {
        AbstractValue::ID
    } else {
        AbstractValue::NonID
    });
}

fn unpack(verifier: &mut IDLeakAnalysis, struct_def: &StructDefinition) {
    // When unpacking, fields of the struct will be pushed to the stack in order.
    // An object whose struct type has key ability must have the first field as "id",
    // representing the ID field of the object. It's the focus of our tracking.
    // The struct_with_key_verifier verifies that the first field must be the id field.
    verifier.stack.pop().unwrap();
    let handle = verifier
        .binary_view
        .struct_handle_at(struct_def.struct_handle);
    verifier.stack.push(if handle.abilities.has_key() {
        AbstractValue::ID
    } else {
        AbstractValue::NonID
    });
    for _ in 1..num_fields(struct_def) {
        verifier.stack.push(AbstractValue::NonID);
    }
}

fn execute_inner(
    verifier: &mut IDLeakAnalysis,
    state: &mut AbstractState,
    bytecode: &Bytecode,
    _: CodeOffset,
) -> SuiResult {
    // TODO: Better dianostics with location
    match bytecode {
        Bytecode::Pop => {
            verifier.stack.pop().unwrap();
        }
        Bytecode::CopyLoc(local) => {
            let value = state.locals.get(local).unwrap();
            verifier.stack.push(*value);
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
        | Bytecode::CastU64
        | Bytecode::CastU128
        | Bytecode::Not
        | Bytecode::VecLen(_)
        | Bytecode::VecPopBack(_) => {
            verifier.stack.pop().unwrap();
            verifier.stack.push(AbstractValue::NonID);
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
            verifier.stack.push(AbstractValue::NonID);
        }
        Bytecode::WriteRef => {
            // Top of stack is the reference, and the second element is the value.
            verifier.stack.pop().unwrap();
            if verifier.stack.pop().unwrap() == AbstractValue::ID {
                return Err(verification_failure("ID is leaked to a reference.".to_string()));
            }
        }

        // These bytecodes produce references, and hence cannot be ID.
        Bytecode::MutBorrowLoc(_)
        | Bytecode::ImmBorrowLoc(_)
        | Bytecode::MutBorrowField(_)
        | Bytecode::MutBorrowFieldGeneric(_)
        | Bytecode::ImmBorrowField(_)
        | Bytecode::ImmBorrowFieldGeneric(_) => {
            verifier.stack.push(AbstractValue::NonID);
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
            for _ in 0..verifier.function_view.return_().len() {
                if verifier.stack.pop().unwrap() == AbstractValue::ID {
                    return Err(verification_failure("ID leaked through function return.".to_string()));
                }
            }
        }

        Bytecode::BrTrue(_) | Bytecode::BrFalse(_) | Bytecode::Abort => {
            verifier.stack.pop().unwrap();
        }

        // These bytecodes produce constants, and hence cannot be ID.
        Bytecode::LdTrue | Bytecode::LdFalse | Bytecode::LdU8(_) | Bytecode::LdU64(_) | Bytecode::LdU128(_) | Bytecode::LdConst(_) => {
            verifier.stack.push(AbstractValue::NonID);
        }

        Bytecode::Pack(idx) => {
            let struct_def = verifier.binary_view.struct_def_at(*idx)?;
            pack(verifier, struct_def);
        }
        Bytecode::PackGeneric(idx) => {
            let struct_inst = verifier.binary_view.struct_instantiation_at(*idx)?;
            let struct_def = verifier.binary_view.struct_def_at(struct_inst.def)?;
            pack(verifier, struct_def);
        }
        Bytecode::Unpack(idx) => {
            let struct_def = verifier.binary_view.struct_def_at(*idx)?;
            unpack(verifier, struct_def);
        }
        Bytecode::UnpackGeneric(idx) => {
            let struct_inst = verifier.binary_view.struct_instantiation_at(*idx)?;
            let struct_def = verifier.binary_view.struct_def_at(struct_inst.def)?;
            unpack(verifier, struct_def);
        }

        Bytecode::VecPack(_, num) => {
            for _ in 0..*num {
                if verifier.stack.pop().unwrap() == AbstractValue::ID {
                    return Err(verification_failure("ID is leaked into a vector".to_string()));
                }
            }
            verifier.stack.push(AbstractValue::NonID);
        }

        Bytecode::VecPushBack(_) => {
            if verifier.stack.pop().unwrap() == AbstractValue::ID {
                return Err(verification_failure("ID is leaked into a vector".to_string()));
            }
            verifier.stack.pop().unwrap();
        }

        Bytecode::VecUnpack(_, num) => {
            verifier.stack.pop().unwrap();

            for _ in 0..*num {
                verifier.stack.push(AbstractValue::NonID);
            }
        }

        Bytecode::VecSwap(_) => {
            verifier.stack.pop().unwrap();
            verifier.stack.pop().unwrap();
            verifier.stack.pop().unwrap();
        }
    };
    Ok(())
}
