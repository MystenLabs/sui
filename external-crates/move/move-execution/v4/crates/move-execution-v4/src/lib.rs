// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Harness glue for the *frozen* v4 Move subsystem (`move-execution/v4`).
//!
//! Frozen from commit `ae60d065ed27576d25ac1c94145e5ffe3ddcf89e` (`origin/main` at cut time).
//!
//! The v4 VM predates the removal of `Type::TyParam`: its `function_information` returns the
//! *declared* signature, with type parameters unsubstituted. The harness contract is
//! post-substitution, so [`MoveRuntimeHarness::function_signature`] adapts here by substituting
//! the (already verified) type arguments -- the same substitution `sui-adapter`'s
//! `subst_signature` used to perform before the contract moved into the VM.

use move_binary_format::{
    CompiledModule,
    errors::{Location, PartialVMError, PartialVMResult, VMResult},
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
        gas_schedule::{Gas, GasStatus, INITIAL_COST_SCHEDULE},
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage,
        vm_arguments::ValueFrame,
        vm_test_adapter::VMTestAdapter,
    },
    execution::{Type, TypeSubst as _, values::Value, vm::MoveVM},
    shared::{gas::UnmeteredGasMeter, linkage_context::LinkageContext},
};

pub struct V4Subsystem;

impl MoveRuntimeHarness for V4Subsystem {
    const SUBSYSTEM_VERSION: MoveExecutionVersion = MoveExecutionVersion::Version(4);

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
        let information = vm.function_information(module, function, ty_args)?;
        // Adaptation to the harness contract: this subsystem returns declared signatures, so
        // substitute the type arguments here (formerly sui-adapter's `subst_signature`).
        let parameters = information
            .parameters
            .into_iter()
            .map(|ty| ty.subst(ty_args))
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|e| e.finish(Location::Undefined))?;
        let return_ = information
            .return_
            .into_iter()
            .map(|ty| ty.subst(ty_args))
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|e| e.finish(Location::Undefined))?;
        Ok(FunctionSignature {
            parameters,
            return_,
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

    fn execute_function_metered(
        vm: &mut Self::Vm<'_>,
        module: &ModuleId,
        function: &IdentStr,
        ty_args: Vec<Self::Type>,
        serialized_args: Vec<Vec<u8>>,
    ) -> VMResult<(Vec<Self::Value>, u64)> {
        const GAS_BUDGET: u64 = 1_000_000;
        let mut gas = GasStatus::new(&INITIAL_COST_SCHEDULE, Gas::new(GAS_BUDGET));
        let frame = ValueFrame::serialized_call(
            vm,
            module,
            function,
            ty_args,
            serialized_args,
            &mut gas,
            None::<&mut MoveTraceBuilder>,
            /* bypass_declared_entry_check */ true,
        )?;
        let gas_used = GAS_BUDGET - u64::from(gas.remaining_gas());
        Ok((frame.values, gas_used))
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

impl MoveVerifierHarness for V4Subsystem {
    fn verify_module(
        module: &CompiledModule,
        config: &VerifierConfig,
        meter: &mut (impl Meter + ?Sized),
    ) -> VMResult<()> {
        move_bytecode_verifier::verify_module_with_config_metered(config, module, meter)
    }
}
