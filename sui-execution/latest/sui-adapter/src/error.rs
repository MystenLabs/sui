// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    errors::{Location, VMError},
    file_format::FunctionDefinitionIndex,
};
use move_core_types::{
    resolver::MoveResolver,
    vm_status::{StatusCode, StatusType},
};
use move_vm_runtime::move_vm::MoveVM;
use sui_types::error::{ExecutionError, SuiError};
use sui_types::execution_status::{ExecutionFailureStatus, MoveLocation, MoveLocationOpt};

pub(crate) fn convert_vm_error<S: MoveResolver<Err = SuiError>>(
    error: VMError,
    vm: &MoveVM,
    state_view: &S,
    resolve_abort_location_to_package_id: bool,
) -> ExecutionError {
    let kind = match (error.major_status(), error.sub_status(), error.location()) {
        (StatusCode::EXECUTED, _, _) => {
            // If we have an error the status probably shouldn't ever be Executed
            debug_assert!(false, "VmError shouldn't ever report successful execution");
            ExecutionFailureStatus::VMInvariantViolation
        }
        (StatusCode::ABORTED, None, _) => {
            debug_assert!(false, "No abort code");
            // this is a Move VM invariant violation, the code should always be there
            ExecutionFailureStatus::VMInvariantViolation
        }
        (StatusCode::ABORTED, Some(code), Location::Module(id)) => {
            let abort_location_id = if resolve_abort_location_to_package_id {
                state_view.relocate(id).unwrap_or_else(|_| id.clone())
            } else {
                id.clone()
            };
            let offset = error.offsets().first().copied().map(|(f, i)| (f.0, i));
            debug_assert!(offset.is_some(), "Move should set the location on aborts");
            let (function, instruction) = offset.unwrap_or((0, 0));
            let function_name = vm.load_module(id, state_view).ok().map(|module| {
                let fdef = module.function_def_at(FunctionDefinitionIndex(function));
                let fhandle = module.function_handle_at(fdef.function);
                module.identifier_at(fhandle.name).to_string()
            });
            ExecutionFailureStatus::MoveAbort(
                MoveLocation {
                    module: abort_location_id,
                    function,
                    instruction,
                    function_name,
                },
                code,
            )
        }
        (StatusCode::OUT_OF_GAS, _, _) => ExecutionFailureStatus::InsufficientGas,
        (_, _, location) => match error.major_status().status_type() {
            StatusType::Execution => {
                debug_assert!(error.major_status() != StatusCode::ABORTED);
                let location = match location {
                    Location::Module(id) => {
                        let offset = error.offsets().first().copied().map(|(f, i)| (f.0, i));
                        debug_assert!(
                            offset.is_some(),
                            "Move should set the location on all execution errors. Error {error}"
                        );
                        let (function, instruction) = offset.unwrap_or((0, 0));
                        let function_name = vm.load_module(id, state_view).ok().map(|module| {
                            let fdef = module.function_def_at(FunctionDefinitionIndex(function));
                            let fhandle = module.function_handle_at(fdef.function);
                            module.identifier_at(fhandle.name).to_string()
                        });
                        Some(MoveLocation {
                            module: id.clone(),
                            function,
                            instruction,
                            function_name,
                        })
                    }
                    _ => None,
                };
                ExecutionFailureStatus::MovePrimitiveRuntimeError(MoveLocationOpt(location))
            }
            StatusType::Validation
            | StatusType::Verification
            | StatusType::Deserialization
            | StatusType::Unknown => ExecutionFailureStatus::VMVerificationOrDeserializationError,
            StatusType::InvariantViolation => ExecutionFailureStatus::VMInvariantViolation,
        },
    };
    ExecutionError::new_with_source(kind, error)
}
