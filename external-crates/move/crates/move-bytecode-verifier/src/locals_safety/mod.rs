// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines the transfer functions for verifying local safety of a procedure body.
//! It is concerned with the assignment state of a local variable at the time of usage, which is
//! a control flow sensitive check.

mod abstract_state;

use crate::{
    absint::{AbstractInterpreter, TransferFunctions},
    locals_safety::abstract_state::{RET_PER_LOCAL_COST, STEP_BASE_COST},
    meter::{Meter, Scope},
};
use abstract_state::{AbstractState, LocalState};
use move_binary_format::{
    binary_views::{BinaryIndexedView, FunctionView},
    errors::{PartialVMError, PartialVMResult},
    file_format::{Bytecode, CodeOffset},
};
use move_core_types::vm_status::StatusCode;

pub(crate) fn verify<'a>(
    resolver: &BinaryIndexedView,
    function_view: &'a FunctionView<'a>,
    meter: &mut impl Meter,
) -> PartialVMResult<()> {
    let initial_state = AbstractState::new(resolver, function_view)?;
    LocalsSafetyAnalysis().analyze_function(initial_state, function_view, meter)
}

fn execute_inner(
    state: &mut AbstractState,
    bytecode: &Bytecode,
    offset: CodeOffset,
    meter: &mut impl Meter,
) -> PartialVMResult<()> {
    meter.add(Scope::Function, STEP_BASE_COST)?;
    match bytecode {
        Bytecode::StLoc(idx) => match state.local_state(*idx) {
            LocalState::MaybeAvailable | LocalState::Available
                if !state.local_abilities(*idx).has_drop() =>
            {
                return Err(state.error(StatusCode::STLOC_UNSAFE_TO_DESTROY_ERROR, offset))
            }
            _ => state.set_available(*idx),
        },

        Bytecode::MoveLoc(idx) => match state.local_state(*idx) {
            LocalState::MaybeAvailable | LocalState::Unavailable => {
                return Err(state.error(StatusCode::MOVELOC_UNAVAILABLE_ERROR, offset))
            }
            LocalState::Available => state.set_unavailable(*idx),
        },

        Bytecode::CopyLoc(idx) => match state.local_state(*idx) {
            LocalState::MaybeAvailable | LocalState::Unavailable => {
                return Err(state.error(StatusCode::COPYLOC_UNAVAILABLE_ERROR, offset))
            }
            LocalState::Available => (),
        },

        Bytecode::MutBorrowLoc(idx) | Bytecode::ImmBorrowLoc(idx) => {
            match state.local_state(*idx) {
                LocalState::Unavailable | LocalState::MaybeAvailable => {
                    return Err(state.error(StatusCode::BORROWLOC_UNAVAILABLE_ERROR, offset))
                }
                LocalState::Available => (),
            }
        }

        Bytecode::Ret => {
            let local_states = state.local_states();
            meter.add_items(Scope::Function, RET_PER_LOCAL_COST, local_states.len())?;
            let all_local_abilities = state.all_local_abilities();
            assert!(local_states.len() == all_local_abilities.len());
            for (local_state, local_abilities) in local_states.iter().zip(all_local_abilities) {
                match local_state {
                    LocalState::MaybeAvailable | LocalState::Available
                        if !local_abilities.has_drop() =>
                    {
                        return Err(
                            state.error(StatusCode::UNSAFE_RET_UNUSED_VALUES_WITHOUT_DROP, offset)
                        )
                    }
                    _ => (),
                }
            }
        }

        Bytecode::Pop
        | Bytecode::BrTrue(_)
        | Bytecode::BrFalse(_)
        | Bytecode::Abort
        | Bytecode::Branch(_)
        | Bytecode::Nop
        | Bytecode::FreezeRef
        | Bytecode::MutBorrowField(_)
        | Bytecode::MutBorrowFieldGeneric(_)
        | Bytecode::ImmBorrowField(_)
        | Bytecode::ImmBorrowFieldGeneric(_)
        | Bytecode::LdU8(_)
        | Bytecode::LdU16(_)
        | Bytecode::LdU32(_)
        | Bytecode::LdU64(_)
        | Bytecode::LdU128(_)
        | Bytecode::LdU256(_)
        | Bytecode::LdConst(_)
        | Bytecode::LdTrue
        | Bytecode::LdFalse
        | Bytecode::Call(_)
        | Bytecode::CallGeneric(_)
        | Bytecode::Pack(_)
        | Bytecode::PackGeneric(_)
        | Bytecode::Unpack(_)
        | Bytecode::UnpackGeneric(_)
        | Bytecode::ReadRef
        | Bytecode::WriteRef
        | Bytecode::CastU8
        | Bytecode::CastU16
        | Bytecode::CastU32
        | Bytecode::CastU64
        | Bytecode::CastU128
        | Bytecode::CastU256
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
        | Bytecode::Not
        | Bytecode::Eq
        | Bytecode::Neq
        | Bytecode::Lt
        | Bytecode::Gt
        | Bytecode::Le
        | Bytecode::Ge
        | Bytecode::MutBorrowGlobal(_)
        | Bytecode::MutBorrowGlobalGeneric(_)
        | Bytecode::ImmBorrowGlobal(_)
        | Bytecode::ImmBorrowGlobalGeneric(_)
        | Bytecode::Exists(_)
        | Bytecode::ExistsGeneric(_)
        | Bytecode::MoveFrom(_)
        | Bytecode::MoveFromGeneric(_)
        | Bytecode::MoveTo(_)
        | Bytecode::MoveToGeneric(_)
        | Bytecode::VecPack(..)
        | Bytecode::VecLen(_)
        | Bytecode::VecImmBorrow(_)
        | Bytecode::VecMutBorrow(_)
        | Bytecode::VecPushBack(_)
        | Bytecode::VecPopBack(_)
        | Bytecode::VecUnpack(..)
        | Bytecode::VecSwap(_)
        | Bytecode::PackVariant(_, _)
        | Bytecode::PackVariantGeneric(_, _)
        | Bytecode::UnpackVariant(_, _)
        | Bytecode::UnpackVariantImmRef(_, _)
        | Bytecode::UnpackVariantMutRef(_, _)
        | Bytecode::UnpackVariantGeneric(_, _)
        | Bytecode::UnpackVariantGenericImmRef(_, _)
        | Bytecode::UnpackVariantGenericMutRef(_, _)
        | Bytecode::VariantSwitch(_) => (),
    };
    Ok(())
}

struct LocalsSafetyAnalysis();

impl TransferFunctions for LocalsSafetyAnalysis {
    type State = AbstractState;
    type Error = PartialVMError;

    fn execute(
        &mut self,
        state: &mut Self::State,
        bytecode: &Bytecode,
        index: CodeOffset,
        _last_index: CodeOffset,
        meter: &mut impl Meter,
    ) -> PartialVMResult<()> {
        execute_inner(state, bytecode, index, meter)
    }
}

impl AbstractInterpreter for LocalsSafetyAnalysis {}
