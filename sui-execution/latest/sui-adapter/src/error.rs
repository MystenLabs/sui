// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    errors::{Location, VMError},
    file_format::FunctionDefinitionIndex,
    CompiledModule,
};
use move_core_types::{
    language_storage::ModuleId,
    resolver::ModuleResolver,
    vm_status::{StatusCode, StatusType},
};
use move_vm_runtime::shared::types::VersionId;
use sui_protocol_config::ProtocolConfig;
use sui_types::{base_types::ObjectID, error::ExecutionError, Identifier};
use sui_types::{
    execution_config_utils::to_binary_config,
    execution_status::{ExecutionFailureStatus, MoveLocation, MoveLocationOpt},
};

use crate::linkage_resolution::ResolvedLinkage;

pub(crate) fn convert_vm_error(
    error: VMError,
    resolution_linkage: &ResolvedLinkage,
    state_view: &impl ModuleResolver,
    protocol_config: &ProtocolConfig,
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
            let version_id = resolution_linkage
                .linkage
                .get(&ObjectID::from(*id.address()))
                .map(|a| **a);

            let abort_location_id = if protocol_config.resolve_abort_locations_to_package_id() {
                version_id.unwrap_or_else(|| *id.address())
            } else {
                *id.address()
            };

            let module_id = ModuleId::new(abort_location_id, id.name().to_owned());
            let offset = error.offsets().first().copied().map(|(f, i)| (f.0, i));
            debug_assert!(offset.is_some(), "Move should set the location on aborts");
            let (function, instruction) = offset.unwrap_or((0, 0));
            let function_name = version_id.and_then(|version_id| {
                load_module_function_name(
                    version_id,
                    id.name().to_owned(),
                    FunctionDefinitionIndex(function),
                    state_view,
                    protocol_config,
                )
            });
            ExecutionFailureStatus::MoveAbort(
                MoveLocation {
                    module: module_id,
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
                        let version_id = resolution_linkage
                            .linkage
                            .get(&ObjectID::from(*id.address()))
                            .map(|a| **a);

                        let function_name = version_id.and_then(|version_id| {
                            load_module_function_name(
                                version_id,
                                id.name().to_owned(),
                                FunctionDefinitionIndex(function),
                                state_view,
                                protocol_config,
                            )
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

fn load_module_function_name(
    package_version_id: VersionId,
    module_name: Identifier,
    function_index: FunctionDefinitionIndex,
    state_view: &impl ModuleResolver,
    protocol_config: &ProtocolConfig,
) -> Option<String> {
    state_view
        .get_module(&ModuleId::new(package_version_id, module_name))
        .ok()
        .flatten()
        .and_then(|m| {
            CompiledModule::deserialize_with_config(&m, &to_binary_config(protocol_config)).ok()
        })
        .map(|module| {
            let fdef = module.function_def_at(function_index);
            let fhandle = module.function_handle_at(fdef.function);
            module.identifier_at(fhandle.name).to_string()
        })
}
