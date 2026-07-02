// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    errors::{Location, VMError},
    file_format::FunctionDefinitionIndex,
};
use move_core_types::{
    language_storage::ModuleId,
    vm_status::{StatusCode, StatusType},
};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::error::ExecutionError;
use sui_types::execution_status::{ExecutionErrorKind, MoveLocation, MoveLocationOpt};
use sui_types::funds_accumulator::FUNDS_ACCUMULATOR_MODULE_NAME;

pub(crate) fn convert_vm_error_impl(
    error: VMError,
    abort_module_id_relocation_fn: &impl Fn(&ModuleId) -> ModuleId,
    function_name_resolution_fn: &impl Fn(&ModuleId, FunctionDefinitionIndex) -> Option<String>,
) -> ExecutionError {
    let kind = match (error.major_status(), error.sub_status(), error.location()) {
        (StatusCode::EXECUTED, _, _) => {
            // If we have an error the status probably shouldn't ever be Executed
            debug_assert!(false, "VmError shouldn't ever report successful execution");
            ExecutionErrorKind::VMInvariantViolation
        }
        (StatusCode::ABORTED, None, _) => {
            debug_assert!(false, "No abort code");
            // this is a Move VM invariant violation, the code should always be there
            ExecutionErrorKind::VMInvariantViolation
        }
        // The abort minted by the `check_sufficient_object_funds` native on an overdraw. The raw
        // code is unambiguous within `0x2::funds_accumulator`: the module's own Move-level aborts
        // use clever-encoded error constants (or `EOverflow = 0`), so a plain
        // `E_OBJECT_FUNDS_INSUFFICIENT` there can only come from the native. Converted to a
        // dedicated kind so clients see a legible status instead of an opaque abort code.
        (StatusCode::ABORTED, Some(code), Location::Module(id))
            if code == sui_move_natives::funds_accumulator::E_OBJECT_FUNDS_INSUFFICIENT
                && id.address() == &SUI_FRAMEWORK_ADDRESS
                && id.name() == FUNDS_ACCUMULATOR_MODULE_NAME =>
        {
            ExecutionErrorKind::InsufficientObjectFundsForWithdraw
        }
        (StatusCode::ABORTED, Some(code), Location::Module(id)) => {
            let abort_location_id = abort_module_id_relocation_fn(id);
            let offset = error.offsets().first().copied().map(|(f, i)| (f.0, i));
            debug_assert!(offset.is_some(), "Move should set the location on aborts");
            let (function, instruction) = offset.unwrap_or((0, 0));
            let function_name = function_name_resolution_fn(id, FunctionDefinitionIndex(function));
            ExecutionErrorKind::MoveAbort(
                MoveLocation {
                    module: abort_location_id,
                    function,
                    instruction,
                    function_name,
                },
                code,
            )
        }
        (StatusCode::OUT_OF_GAS, _, _) => ExecutionErrorKind::InsufficientGas,
        // The unwind of a system-object-unavailable retry request (see
        // `TemporaryStore::check_system_object_available`). A node-local, transient condition: the
        // authority discards the produced effects and re-enqueues the transaction, so this kind
        // never reaches committed effects. Mapped to a dedicated kind so the unwind can be
        // recognized by `.kind()` without inspecting the source `VMError`.
        (StatusCode::SYSTEM_OBJECT_NOT_AVAILABLE_LOCALLY, _, _) => {
            ExecutionErrorKind::SystemObjectNotAvailableLocally
        }
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
                        let function_name =
                            function_name_resolution_fn(id, FunctionDefinitionIndex(function));
                        Some(MoveLocation {
                            module: id.clone(),
                            function,
                            instruction,
                            function_name,
                        })
                    }
                    _ => None,
                };
                ExecutionErrorKind::MovePrimitiveRuntimeError(MoveLocationOpt(location))
            }
            StatusType::Validation
            | StatusType::Verification
            | StatusType::Deserialization
            | StatusType::Unknown => ExecutionErrorKind::VMVerificationOrDeserializationError,
            StatusType::InvariantViolation => ExecutionErrorKind::VMInvariantViolation,
        },
    };
    ExecutionError::new_with_source(kind, error)
}
