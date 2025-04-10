// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::identifier_interner::{intern_ident_str, intern_identifier},
    dbg_println,
    execution::{
        dispatch_tables::VMDispatchTables,
        interpreter::{self, locals::BaseHeap},
    },
    jit::execution::ast::{Function, Type, TypeSubst},
    natives::extensions::NativeContextExtensions,
    record_time,
    runtime::telemetry::{TelmetryContext, TransactionTelemetryContext},
    shared::{
        gas::GasMeter,
        linkage_context::LinkageContext,
        serialization::{SerializedReturnValues, *},
        vm_pointer::VMPointer,
    },
    with_transaction_telemetry,
};
use move_binary_format::{
    errors::{Location, PartialVMError, PartialVMResult, VMError, VMResult},
    file_format::{AbilitySet, CodeOffset, FunctionDefinitionIndex, LocalIndex, Visibility},
};
use move_core_types::{
    annotated_value,
    identifier::IdentStr,
    language_storage::{ModuleId, TypeTag},
    runtime_value,
    vm_status::{StatusCode, StatusType},
};
use move_trace_format::format::MoveTraceBuilder;
use move_vm_config::runtime::VMConfig;
use std::{borrow::Borrow, sync::Arc};

use super::{
    dispatch_tables::{IntraPackageKey, VirtualTableKey},
    tracing::tracer::VMTracer,
};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// A runnable Instance of a Virtual Machine. This is an instance with respect to some DataStore,
/// holding the Runtime VTables for that data store in order to invoke functions from it. This
/// instance is the main "execution" context for a virtual machine, allowing calls to
/// `execute_function` to run Move code located in the VM Cache.
///
/// Note this does NOT support publication. See `vm.rs` for publication.
#[allow(dead_code)]
pub struct MoveVM<'extensions> {
    /// The VM cache
    pub(crate) virtual_tables: VMDispatchTables,
    /// The linkage context used to create this VM instance
    pub(crate) link_context: LinkageContext,
    /// Native context extensions for the interpreter
    pub(crate) native_extensions: NativeContextExtensions<'extensions>,
    /// The Move VM's configuration.
    pub(crate) vm_config: Arc<VMConfig>,
    /// The Move Runtime telemetry
    pub(crate) telemetry: Arc<TelmetryContext>,
    /// Move VM Base Heap, which holds base arguments, including reference return values, etc.
    pub(crate) base_heap: BaseHeap,
}

pub(crate) struct MoveVMFunction {
    function: VMPointer<Function>,
    pub(crate) parameters: Vec<Type>,
    pub(crate) return_type: Vec<Type>,
}

/// Externally visibile information about a function that can be asked and the VM will answer.
pub struct LoadedFunctionInformation {
    pub is_entry: bool,
    pub is_native: bool,
    pub visibility: Visibility,
    pub index: FunctionDefinitionIndex,
    pub instruction_count: CodeOffset,
    pub parameters: Vec<Type>,
    pub return_: Vec<Type>,
}

impl<'extensions> MoveVM<'extensions> {
    // -------------------------------------------
    // Entry Points
    // -------------------------------------------

    /// Execute a Move function with the given arguments. This is mainly designed for an external
    /// environment to invoke system logic written in Move.
    ///
    /// NOTE: There are NO checks on the `args` except that they can deserialize into the provided
    /// types.
    /// The ability to deserialize `args` into arbitrary types is *very* powerful, e.g. it can
    /// used to manufacture `signer`'s or `Coin`'s from raw bytes. It is the responsibility of the
    /// caller (e.g. adapter) to ensure that this power is used responsibly/securely for its
    /// use-case.
    ///
    /// The caller MUST ensure
    ///   - All types and modules referred to by the type arguments exist.
    ///   - The signature is valid for the rules of the adapter
    ///
    /// The Move VM MUST return an invariant violation if the caller fails to follow any of the
    /// rules above.
    ///
    /// The VM will check that the function is marked as an 'entry' function.
    ///
    /// Currently if any other error occurs during execution, the Move VM will simply propagate that
    /// error back to the outer environment without handling/translating it. This behavior may be
    /// revised in the future.
    ///
    /// In case an invariant violation occurs, the whole Session should be considered corrupted and
    /// one shall not proceed with effect generation.
    pub fn execute_entry_function(
        &mut self,
        module: &ModuleId,
        function_name: &IdentStr,
        ty_args: Vec<Type>,
        args: Vec<impl Borrow<[u8]>>,
        gas_meter: &mut impl GasMeter,
    ) -> VMResult<SerializedReturnValues> {
        let bypass_declared_entry_check = false;
        self.execute_function(
            module,
            function_name,
            ty_args,
            args,
            None,
            gas_meter,
            bypass_declared_entry_check,
        )
    }

    /// Similar to execute_entry_function, but it bypasses visibility checks and accepts a tracer
    pub fn execute_function_bypass_visibility(
        &mut self,
        module: &ModuleId,
        function_name: &IdentStr,
        ty_args: Vec<Type>,
        args: Vec<impl Borrow<[u8]>>,
        gas_meter: &mut impl GasMeter,
        tracer: Option<&mut MoveTraceBuilder>,
    ) -> VMResult<SerializedReturnValues> {
        move_vm_profiler::tracing_feature_enabled! {
            use move_vm_profiler::GasProfiler;
            if gas_meter.get_profiler_mut().is_none() {
                gas_meter.set_profiler(GasProfiler::init_default_cfg(
                    function_name.to_string(),
                    gas_meter.remaining_gas().into(),
                ));
            }
        }

        let tracer = if cfg!(feature = "tracing") {
            tracer
        } else {
            None
        };

        dbg_println!("running {module}::{function_name}");
        dbg_println!("tables: {:#?}", self.virtual_tables.loaded_packages);
        let bypass_declared_entry_check = true;
        self.execute_function(
            module,
            function_name,
            ty_args,
            args,
            tracer,
            gas_meter,
            bypass_declared_entry_check,
        )
    }

    // -------------------------------------------
    // External Queries
    // -------------------------------------------

    /// NB: The `module_id` is using the _runtime_ ID.
    pub fn function_information(
        &self,
        module_id: &ModuleId,
        function_name: &IdentStr,
        ty_args: &[Type],
    ) -> VMResult<LoadedFunctionInformation> {
        let MoveVMFunction {
            function,
            parameters,
            return_type,
        } = self.find_function(module_id, function_name, ty_args)?;

        Ok(LoadedFunctionInformation {
            is_entry: function.to_ref().is_entry,
            is_native: function.to_ref().def_is_native,
            visibility: function.to_ref().visibility,
            index: function.to_ref().index,
            instruction_count: function.to_ref().code.len() as CodeOffset,
            parameters,
            return_: return_type,
        })
    }

    pub fn vm_config(&self) -> &move_vm_config::runtime::VMConfig {
        &self.vm_config
    }

    pub fn type_abilities(&self, ty: &Type) -> VMResult<AbilitySet> {
        self.virtual_tables
            .abilities(ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    /// Return a `TypeTag` for the provided `Type` using the VM's virtual tables. The returned
    /// `TypeTag` will use defining type IDs.
    pub fn type_tag_for_type_defining_ids(&self, ty: &Type) -> VMResult<TypeTag> {
        self.virtual_tables
            .type_to_type_tag(ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    /// Resolve a `TypeTag` to a `Type` using the VM's virtual tables.
    /// NB: the `TypeTag` is coming from outside and therefore _may_ not represent a valid type
    /// (e.g., an undefined type). Therefore any errors in resolution should be treated as runtime
    /// errors and not anything else.
    /// Additionally, the type tag _must_ use defining type IDs. Original/runtime IDs (or package
    /// IDs) are not correct here.
    pub fn load_type(&self, tag: &TypeTag) -> VMResult<Type> {
        self.virtual_tables
            .load_type(tag)
            .map_err(|e| Self::convert_to_external_type_tag_error(e, tag))
    }

    /// Resolve a `TypeTag` to a runtime type layout using the VM's virtual tables.
    /// NB: the `TypeTag` is coming from outside and therefore _may_ not represent a valid type
    /// (e.g., an undefined type). Therefore any errors in resolution should be treated as runtime
    /// errors and not anything else.
    /// Additionally, the type tag _must_ use defining type IDs. Original/runtime IDs (or package
    /// IDs) are not correct here.
    pub fn runtime_type_layout(&self, ty: &TypeTag) -> VMResult<runtime_value::MoveTypeLayout> {
        self.virtual_tables
            .get_type_layout(ty)
            .map_err(|e| Self::convert_to_external_type_tag_error(e, ty))
    }

    /// Resolve a `TypeTag` to an annotated type layout using the VM's virtual tables.
    /// NB: the `TypeTag` is coming from outside and therefore _may_ not represent a valid type
    /// (e.g., an undefined type). Therefore any errors in resolution should be treated as runtime
    /// errors and not anything else.
    /// Additionally, the type tag _must_ use defining type IDs. Original/runtime IDs (or package
    /// IDs) are not correct here.
    pub fn annotated_type_layout(&self, ty: &TypeTag) -> VMResult<annotated_value::MoveTypeLayout> {
        self.virtual_tables
            .get_fully_annotated_type_layout(ty)
            .map_err(|e| Self::convert_to_external_type_tag_error(e, ty))
    }

    fn convert_to_external_type_tag_error(err: VMError, tag: &TypeTag) -> VMError {
        if err.major_status().status_type() == StatusType::InvariantViolation {
            PartialVMError::new(StatusCode::EXTERNAL_RESOLUTION_REQUEST_ERROR)
                .with_message(format!(
                    "Failed to resolve external type tag{tag}{}",
                    err.message()
                        .map(|s| format!(": {}", s))
                        .unwrap_or_default()
                ))
                .finish(Location::Undefined)
        } else {
            err
        }
    }

    /// Return the linkage context of the VM.
    pub fn linkage_context(&self) -> &LinkageContext {
        &self.link_context
    }

    // -------------------------------------------
    // Execution Operations
    // -------------------------------------------

    /// Entry point for function execution, allowing an instance to run the specified function.
    /// Note that the specified module is a `runtime_id`, meaning it should already be resolved
    /// with respect to the linkage context.
    fn execute_function(
        &mut self,
        runtime_id: &ModuleId,
        function_name: &IdentStr,
        type_arguments: Vec<Type>,
        serialized_args: Vec<impl Borrow<[u8]>>,
        tracer: Option<&mut MoveTraceBuilder>,
        gas_meter: &mut impl GasMeter,
        bypass_declared_entry_check: bool,
    ) -> VMResult<SerializedReturnValues> {
        let telemetry = Arc::clone(&self.telemetry);
        with_transaction_telemetry!(
            telemetry;
            txn_telemetry => {
                record_time!(EXECUTION; txn_telemetry => {
                        // Find the function definition
                        let MoveVMFunction {
                            function,
                            parameters,
                            return_type,
                        } = self.find_function(runtime_id, function_name, &type_arguments)?;

                        if !bypass_declared_entry_check && !function.to_ref().is_entry {
                            return Err(PartialVMError::new(
                                StatusCode::EXECUTE_ENTRY_FUNCTION_CALLED_ON_NON_ENTRY_FUNCTION,
                            )
                            .finish(Location::Undefined));
                        }

                        // execute the function
                        self.execute_function_impl(
                            &mut txn_telemetry,
                            function,
                            type_arguments,
                            parameters,
                            return_type,
                            serialized_args,
                            &mut tracer.map(VMTracer::new),
                            gas_meter,
                        )
                    }
                )
            }
        )
    }

    /// Find the function definition int the specified module and return the information required
    /// to do final verification and execution.
    pub(crate) fn find_function(
        &self,
        // This is expected to be the translated version of the module ID, already translated by
        // the link context. See `sui-adapter/src/programmable_transactions/execution.rs`
        runtime_id: &ModuleId,
        function_name: &IdentStr,
        ty_args: &[Type],
    ) -> VMResult<MoveVMFunction> {
        let (package_key, module_id) = runtime_id.clone().into();
        let module_name =
            intern_identifier(&module_id).map_err(|err| err.finish(Location::Undefined))?;
        let member_name =
            intern_ident_str(function_name).map_err(|err| err.finish(Location::Undefined))?;
        let vtable_key = VirtualTableKey {
            package_key,
            inner_pkg_key: IntraPackageKey {
                module_name,
                member_name,
            },
        };
        let function = self
            .virtual_tables
            .resolve_function(&vtable_key)
            .map_err(|err| err.finish(Location::Undefined))?;

        let fun_ref = function.to_ref();

        let parameters = fun_ref.parameters.iter().map(|ty| ty.to_type()).collect();

        let return_ = fun_ref.return_.iter().map(|ty| ty.to_type()).collect();

        // verify type arguments
        self.virtual_tables
            .verify_ty_args(fun_ref.type_parameters(), ty_args)
            .map_err(|e| e.finish(Location::Module(runtime_id.clone())))?;

        let function = MoveVMFunction {
            function,
            parameters,
            return_type: return_,
        };
        Ok(function)
    }

    /// Perform the actual execution, including setting up the interpreter machine, running the
    /// interpreter, and serializing the return value(s).
    fn execute_function_impl(
        &mut self,
        txn_telemetry: &mut TransactionTelemetryContext,
        func: VMPointer<Function>,
        ty_args: Vec<Type>,
        param_types: Vec<Type>,
        return_types: Vec<Type>,
        serialized_args: Vec<impl Borrow<[u8]>>,
        tracer: &mut Option<VMTracer<'_>>,
        gas_meter: &mut impl GasMeter,
    ) -> VMResult<SerializedReturnValues> {
        let arg_types = param_types
            .into_iter()
            .map(|ty| ty.subst(&ty_args))
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|err| err.finish(Location::Undefined))?;
        let mut_ref_args = arg_types
            .iter()
            .enumerate()
            .filter_map(|(idx, ty)| match ty {
                Type::MutableReference(inner) => Some((idx, inner.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        // TODO: Lift deserialization out to the PTB layer, and expose the Base Heap to that layer
        // so that it can allocate values into it for usage in function calls.
        // The external calls in should eventually just call `Value`; serialization and
        // deserialization should be done outside of the VM calls.
        let (ref_ids, deserialized_args) = deserialize_args(
            &self.virtual_tables,
            &mut self.base_heap,
            arg_types,
            serialized_args,
        )
        .map_err(|e| e.finish(Location::Undefined))?;
        let return_types = return_types
            .into_iter()
            .map(|ty| ty.subst(&ty_args))
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|err| err.finish(Location::Undefined))?;

        let return_values = interpreter::run(
            txn_telemetry,
            self.vm_config.clone(),
            &mut self.native_extensions,
            tracer,
            gas_meter,
            &mut self.virtual_tables,
            func,
            ty_args,
            deserialized_args,
        )?;

        let serialized_return_values =
            serialize_return_values(&self.virtual_tables, &return_types, return_values)
                .map_err(|e| e.finish(Location::Undefined))?;
        let serialized_mut_ref_outputs = mut_ref_args
            .into_iter()
            .map(|(ndx, ty)| {
                let heap_ref_id = ref_ids.get(&ndx).expect("No heap ref for ref argument");
                // take the value of each reference; return values first in the case that a value
                // points into this local
                let local_val = self.base_heap.take_loc(*heap_ref_id)?;
                let (bytes, layout) = serialize_return_value(&self.virtual_tables, &ty, local_val)?;
                Ok((ndx as LocalIndex, bytes, layout))
            })
            .collect::<PartialVMResult<_>>()
            .map_err(|e| e.finish(Location::Undefined))?;

        Ok(SerializedReturnValues {
            mutable_reference_outputs: serialized_mut_ref_outputs,
            return_values: serialized_return_values,
        })
    }

    // -------------------------------------------
    // Into Methods
    // -------------------------------------------

    pub fn into_extensions(self) -> NativeContextExtensions<'extensions> {
        self.native_extensions
    }

    pub fn extensions(&self) -> &NativeContextExtensions<'extensions> {
        &self.native_extensions
    }
}

impl std::fmt::Debug for MoveVM<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MoveVM")
            .field("virtual_tables", &self.virtual_tables)
            .field("link_context", &self.link_context)
            .field("vm_config", &self.vm_config)
            .field("base_heap", &self.base_heap)
            // Note: native_extensions is intentionally omitted
            .finish()
    }
}
