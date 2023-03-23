// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{Location, PartialVMError, PartialVMResult, VMResult};
use move_binary_format::file_format::AbilitySet;
use move_binary_format::CompiledModule;
use move_bytecode_verifier::VerifierConfig;
use move_core_types::account_address::AccountAddress;
use move_core_types::gas_algebra::NumBytes;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{ModuleId, TypeTag};
use move_core_types::resolver::MoveResolver;
use move_core_types::value::MoveTypeLayout;
use move_core_types::vm_status::StatusCode;
use move_vm_runtime::native_extensions::NativeContextExtensions;
use move_vm_runtime::native_functions::NativeFunctionTable;
use move_vm_runtime::session::{LoadedFunctionInstantiation, SerializedReturnValues};
use move_vm_runtime::{
    config::{VMConfig, VMRuntimeLimitsConfig},
    runtime::VMRuntime as MoveVMInternal,
};
use move_vm_types::data_store::DataStore;
use move_vm_types::gas::GasMeter;
use move_vm_types::loaded_data::runtime_types::{CachedStructIndex, StructType, Type};
use move_vm_types::values::{GlobalValue, Value};
use std::borrow::Borrow;
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_types::error::SuiError;

pub struct MoveVM {
    pub(crate) runtime: MoveVMInternal,
}

impl MoveVM {
    pub fn new(
        natives: NativeFunctionTable,
        protocol_config: &ProtocolConfig,
    ) -> Result<MoveVM, SuiError> {
        let runtime = MoveVMInternal::new(
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

    pub(crate) fn load_module<'r, S: MoveResolver>(
        &self,
        module_id: &ModuleId,
        remote: &'r S,
    ) -> VMResult<Arc<CompiledModule>> {
        let data_store = ObjectRuntimeStore::new(remote);
        self.runtime
            .loader()
            .load_module_public(module_id, &data_store)
    }

    pub(crate) fn load_type<'r, S: MoveResolver>(
        &self,
        type_tag: &TypeTag,
        remote: &'r S,
    ) -> VMResult<Type> {
        let data_store = ObjectRuntimeStore::new(remote);
        self.runtime.loader().load_type(type_tag, &data_store)
    }

    pub(crate) fn get_type_layout<'r, S: MoveResolver>(
        &self,
        type_tag: &TypeTag,
        remote: &'r S,
    ) -> VMResult<MoveTypeLayout> {
        let data_store = ObjectRuntimeStore::new(remote);
        self.runtime.loader().get_type_layout(type_tag, &data_store)
    }

    pub(crate) fn get_type_tag(&self, ty: &Type) -> VMResult<TypeTag> {
        self.runtime
            .loader()
            .type_to_type_tag(ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    pub(crate) fn get_type_abilities(&self, ty: &Type) -> VMResult<AbilitySet> {
        self.runtime
            .loader()
            .abilities(ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    pub(crate) fn get_struct_type(&self, index: CachedStructIndex) -> Option<Arc<StructType>> {
        self.runtime.loader().get_struct_type(index)
    }

    pub(crate) fn load_function<'r, S: MoveResolver>(
        &self,
        module_id: &ModuleId,
        function_name: &IdentStr,
        type_arguments: &[TypeTag],
        remote: &'r S,
    ) -> VMResult<LoadedFunctionInstantiation> {
        let data_store = ObjectRuntimeStore::new(remote);
        self.runtime.loader().load_function_public(
            module_id,
            function_name,
            type_arguments,
            &data_store,
        )
    }

    pub(crate) fn execute_function_bypass_visibility<'r, S: MoveResolver>(
        &self,
        module: &ModuleId,
        function_name: &IdentStr,
        ty_args: Vec<TypeTag>,
        args: Vec<impl Borrow<[u8]>>,
        gas_meter: &mut impl GasMeter,
        remote: &'r S,
        extensions: &mut NativeContextExtensions,
    ) -> VMResult<SerializedReturnValues> {
        let mut data_store = ObjectRuntimeStore::new(remote);
        let bypass_declared_entry_check = true;
        self.runtime.execute_function(
            module,
            function_name,
            ty_args,
            args,
            &mut data_store,
            gas_meter,
            extensions,
            bypass_declared_entry_check,
        )
    }

    pub(crate) fn publish_module_bundle<'r, S: MoveResolver>(
        &self,
        modules: Vec<Vec<u8>>,
        sender: AccountAddress,
        gas_meter: &mut impl GasMeter,
        remote: &'r S,
    ) -> VMResult<()> {
        let mut data_store = ObjectRuntimeStore::new(remote);
        self.runtime
            .publish_module_bundle(modules, sender, &mut data_store, gas_meter)
    }
}

struct ObjectRuntimeStore<'r, S: MoveResolver> {
    remote: &'r S,
}

impl<'r, S: MoveResolver> ObjectRuntimeStore<'r, S> {
    fn new(remote: &'r S) -> Self {
        Self { remote }
    }
}

impl<'r, S: MoveResolver> DataStore for ObjectRuntimeStore<'r, S> {
    fn load_resource(
        &mut self,
        _addr: AccountAddress,
        _ty: &Type,
    ) -> PartialVMResult<(&mut GlobalValue, Option<Option<NumBytes>>)> {
        panic!("should never come here")
    }

    fn link_context(&self) -> AccountAddress {
        self.remote.link_context()
    }

    fn relocate(&self, module_id: &ModuleId) -> PartialVMResult<ModuleId> {
        self.remote
            .relocate(module_id)
            .map_err(|_err| PartialVMError::new(StatusCode::STORAGE_ERROR))
    }

    fn load_module(&self, module_id: &ModuleId) -> VMResult<Vec<u8>> {
        match self.remote.get_module(module_id) {
            Ok(module) => match module {
                Some(module) => Ok(module),
                None => {
                    Err(PartialVMError::new(StatusCode::STORAGE_ERROR).finish(Location::Undefined))
                }
            },
            Err(_err) => {
                Err(PartialVMError::new(StatusCode::STORAGE_ERROR).finish(Location::Undefined))
            }
        }
    }

    fn publish_module(&mut self, _module_id: &ModuleId, _blob: Vec<u8>) -> VMResult<()> {
        Ok(())
    }

    fn emit_event(
        &mut self,
        _guid: Vec<u8>,
        _seq_num: u64,
        _ty: Type,
        _val: Value,
    ) -> PartialVMResult<()> {
        panic!("should never come here")
    }

    fn events(&self) -> &Vec<(Vec<u8>, u64, Type, MoveTypeLayout, Value)> {
        panic!("should never come here")
    }
}
