// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Harness glue for the *latest* (tip) Move subsystem: the in-development `move-vm-runtime`
//! and `move-bytecode-verifier` at `external-crates/move/crates`. The tip carries no version
//! number until it is frozen; it is selected as `MoveExecutionVersion::Latest`.
//!
//! The tip VM's `Type` has no type-parameter variant and `function_information` substitutes type
//! arguments internally, so this glue implements the harness contract directly, with no
//! adaptation.

use move_binary_format::{
    CompiledModule,
    errors::{Location, PartialVMError, VMResult},
};
use move_bytecode_verifier_meter::Meter;
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, TypeTag},
    vm_status::StatusCode,
};
use move_execution_api::{
    FunctionSignature, Linkage, MoveExecutionVersion, MoveRuntimeHarness, MoveVerifierHarness,
};
use move_trace_format::format::MoveTraceBuilder;
use move_vm_config::verifier::VerifierConfig;
use move_vm_runtime::{
    dev_utils::{
        in_memory_test_adapter::InMemoryTestAdapter, storage::StoredPackage,
        vm_arguments::ValueFrame, vm_test_adapter::VMTestAdapter,
    },
    execution::{Type, values::Value, vm::MoveVM},
    shared::{gas::UnmeteredGasMeter, linkage_context::LinkageContext},
};

pub struct LatestSubsystem;

impl MoveRuntimeHarness for LatestSubsystem {
    const SUBSYSTEM_VERSION: MoveExecutionVersion = MoveExecutionVersion::Latest;

    type Runtime = InMemoryTestAdapter;
    type Vm<'e> = MoveVM<'e>;
    type Type = Type;
    type Value = Value;

    fn new_runtime() -> Self::Runtime {
        InMemoryTestAdapter::new()
    }

    fn publish_package(
        runtime: &mut Self::Runtime,
        version_id: AccountAddress,
        modules: Vec<CompiledModule>,
    ) -> VMResult<()> {
        let pkg = StoredPackage::from_modules_for_testing(version_id, modules).map_err(|e| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message(e.to_string())
                .finish(Location::Undefined)
        })?;
        let original_id = pkg.0.original_id;
        VMTestAdapter::publish_package(runtime, original_id, pkg.into_serialized_package())
    }

    fn make_vm<'e>(runtime: &Self::Runtime, linkage: &Linkage) -> VMResult<Self::Vm<'e>> {
        let context = LinkageContext::new(linkage.linkage_table.clone())?;
        runtime.make_vm(context)
    }

    fn load_type(vm: &Self::Vm<'_>, tag: &TypeTag) -> VMResult<Self::Type> {
        vm.load_type(tag)
    }

    fn type_tag(vm: &Self::Vm<'_>, ty: &Self::Type) -> VMResult<TypeTag> {
        vm.type_tag_for_type_defining_ids(ty)
    }

    fn function_signature(
        vm: &Self::Vm<'_>,
        module: &ModuleId,
        function: &IdentStr,
        ty_args: &[Self::Type],
    ) -> VMResult<FunctionSignature<Self::Type>> {
        // The tip VM already returns substituted signatures -- this is the harness contract.
        let information = vm.function_information(module, function, ty_args)?;
        Ok(FunctionSignature {
            parameters: information.parameters,
            return_: information.return_,
        })
    }

    fn execute_function(
        vm: &mut Self::Vm<'_>,
        module: &ModuleId,
        function: &IdentStr,
        ty_args: Vec<Self::Type>,
        serialized_args: Vec<Vec<u8>>,
    ) -> VMResult<Vec<Self::Value>> {
        let frame = ValueFrame::serialized_call(
            vm,
            module,
            function,
            ty_args,
            serialized_args,
            &mut UnmeteredGasMeter,
            None::<&mut MoveTraceBuilder>,
            /* bypass_declared_entry_check */ true,
        )?;
        Ok(frame.values)
    }

    fn serialize_value(vm: &Self::Vm<'_>, value: &Self::Value, ty: &TypeTag) -> VMResult<Vec<u8>> {
        let layout = vm.runtime_type_layout(ty)?;
        value.typed_serialize(&layout).ok_or_else(|| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message(format!("failed to serialize value of type {ty}"))
                .finish(Location::Undefined)
        })
    }
}

impl MoveVerifierHarness for LatestSubsystem {
    fn verify_module(
        module: &CompiledModule,
        config: &VerifierConfig,
        meter: &mut (impl Meter + ?Sized),
    ) -> VMResult<()> {
        move_bytecode_verifier::verify_module_with_config_metered(config, module, meter)
    }
}
