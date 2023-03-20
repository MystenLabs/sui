// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::VMResult;
use move_binary_format::CompiledModule;
use move_bytecode_verifier::VerifierConfig;
use move_core_types::language_storage::ModuleId;
use move_core_types::resolver::MoveResolver;
use move_vm_runtime::native_extensions::NativeContextExtensions;
use move_vm_runtime::native_functions::NativeFunctionTable;
use move_vm_runtime::session::Session;
use move_vm_runtime::{
    config::{VMConfig, VMRuntimeLimitsConfig},
    move_vm::MoveVM as MoveVMInternal,
};
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_types::error::SuiError;

pub struct MoveVM {
    runtime: MoveVMInternal,
}

impl MoveVM {
    pub fn new(
        natives: NativeFunctionTable,
        protocol_config: &ProtocolConfig,
    ) -> Result<MoveVM, SuiError> {
        let runtime = MoveVMInternal::new_with_config(
            natives,
            VMConfig {
                verifier: VerifierConfig {
                    max_loop_depth: Some(protocol_config.max_loop_depth() as usize),
                    max_generic_instantiation_length: Some(
                        protocol_config.max_generic_instantiation_length() as usize,
                    ),
                    max_function_parameters: Some(
                        protocol_config.max_function_parameters() as usize
                    ),
                    max_basic_blocks: Some(protocol_config.max_basic_blocks() as usize),
                    max_value_stack_size: protocol_config.max_value_stack_size() as usize,
                    max_type_nodes: Some(protocol_config.max_type_nodes() as usize),
                    max_push_size: Some(protocol_config.max_push_size() as usize),
                    max_dependency_depth: Some(protocol_config.max_dependency_depth() as usize),
                    max_fields_in_struct: Some(protocol_config.max_fields_in_struct() as usize),
                    max_function_definitions: Some(
                        protocol_config.max_function_definitions() as usize
                    ),
                    max_struct_definitions: Some(protocol_config.max_struct_definitions() as usize),
                    max_constant_vector_len: Some(protocol_config.max_move_vector_len()),

                    max_back_edges_per_function: None,
                    max_back_edges_per_module: None,
                    max_basic_blocks_in_script: None,
                    max_per_fun_meter_units: None,
                    max_per_mod_meter_units: None,
                },
                max_binary_format_version: protocol_config.move_binary_format_version(),
                paranoid_type_checks: false,
                runtime_limits_config: VMRuntimeLimitsConfig {
                    vector_len_max: protocol_config.max_move_vector_len(),
                },
            },
        )
        .map_err(|_| SuiError::ExecutionInvariantViolation)?;
        Ok(MoveVM { runtime })
    }

    pub(crate) fn new_session_with_extensions<'r, S: MoveResolver>(
        &self,
        remote: &'r S,
        extensions: NativeContextExtensions<'r>,
    ) -> Session<'r, '_, S> {
        self.runtime.new_session_with_extensions(remote, extensions)
    }

    pub fn load_module<'r, S: MoveResolver>(
        &self,
        module_id: &ModuleId,
        remote: &'r S,
    ) -> VMResult<Arc<CompiledModule>> {
        self.runtime.load_module(module_id, remote)
    }
}
