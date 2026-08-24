// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The harness interface between the Sui execution layer and a versioned *Move subsystem* (a VM
//! plus its bytecode verifier, cut whole into `move-execution/vN`).
//!
//! A consumer written against these traits is subsystem-agnostic: every registered subsystem
//! version implements both harnesses, and a protocol-config value (`move_execution_version`)
//! selects which one runs. The traits are monomorphized -- selecting a subsystem happens once,
//! at executor construction; execution itself pays nothing for the abstraction.
//!
//! Contract direction: the harness always speaks the *newest* subsystem's contract, and older
//! subsystems' glue crates adapt to it. Concretely here: [`MoveRuntimeHarness::function_signature`]
//! returns signatures with type arguments already substituted (and [`MoveRuntimeHarness::Type`]
//! never contains a type parameter); the frozen v4 subsystem, whose VM returns declared
//! signatures, performs the substitution in its glue.
//!
//! NOTE: This crate is a mock of the `move-execution-api` plan, sized to demonstrate the
//! architecture. The full API additionally carries the value/heap vocabulary and the complete
//! natives-facing surface (`NativeContext` operations, native registration) so that
//! `sui-move-natives` stays a single generic crate, hoists the shared data types
//! (`LinkageContext`, the verification AST, package types) rather than mocking them (see
//! [`Linkage`]), and exposes the verifier's abstract-interpretation framework for the one
//! sui-verifier pass that needs it (`id_leak_verifier`). Those surfaces are elided here.

use std::collections::BTreeMap;

use move_binary_format::{CompiledModule, errors::VMResult};
use move_bytecode_verifier_meter::Meter;
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, TypeTag},
};
use move_vm_config::verifier::VerifierConfig;

/// A linkage: the mapping from original (runtime) package IDs to the package versions that
/// satisfy them for one execution.
///
/// In the full API this is a *hoisted* concrete type -- the VM's `LinkageContext` moved here and
/// shared by every subsystem -- rather than a mock that each glue crate converts from.
#[derive(Debug, Clone, Default)]
pub struct Linkage {
    pub linkage_table: BTreeMap<AccountAddress, AccountAddress>,
}

/// A function signature in a subsystem's type vocabulary. Per the harness contract the types are
/// fully substituted: no type parameter survives in `parameters` or `return_`.
#[derive(Debug, Clone)]
pub struct FunctionSignature<T> {
    pub parameters: Vec<T>,
    pub return_: Vec<T>,
}

/// The execution facet of a Move subsystem: runtime construction, package publication, per-linkage
/// VM instances, type resolution, and function execution.
pub trait MoveRuntimeHarness {
    /// The `move-execution/` version this harness serves (the value `move_execution_version`
    /// dispatches on).
    const SUBSYSTEM_VERSION: u64;

    /// The long-lived runtime: package cache plus natives. (In this mock, the subsystem's
    /// in-memory test adapter, which bundles the runtime with a package store.)
    type Runtime;
    /// A per-linkage VM instance created from the runtime.
    type Vm<'e>;
    /// The subsystem's runtime type vocabulary. Always fully instantiated: no type-parameter
    /// variant crosses this boundary.
    type Type: Clone + std::fmt::Debug;
    /// The subsystem's runtime value representation.
    type Value;

    fn new_runtime() -> Self::Runtime;

    /// Verify-and-publish a package of modules at `version_id` into the runtime's store.
    fn publish_package(
        runtime: &mut Self::Runtime,
        version_id: AccountAddress,
        modules: Vec<CompiledModule>,
    ) -> VMResult<()>;

    fn make_vm<'e>(runtime: &Self::Runtime, linkage: &Linkage) -> VMResult<Self::Vm<'e>>;

    fn load_type(vm: &Self::Vm<'_>, tag: &TypeTag) -> VMResult<Self::Type>;

    /// Render a subsystem type back into the shared `TypeTag` vocabulary (with defining IDs).
    fn type_tag(vm: &Self::Vm<'_>, ty: &Self::Type) -> VMResult<TypeTag>;

    /// The signature of `module::function` instantiated at `ty_args`, substituted: the returned
    /// types contain no type parameters.
    fn function_signature(
        vm: &Self::Vm<'_>,
        module: &ModuleId,
        function: &IdentStr,
        ty_args: &[Self::Type],
    ) -> VMResult<FunctionSignature<Self::Type>>;

    fn execute_function(
        vm: &mut Self::Vm<'_>,
        module: &ModuleId,
        function: &IdentStr,
        ty_args: Vec<Self::Type>,
        serialized_args: Vec<Vec<u8>>,
    ) -> VMResult<Vec<Self::Value>>;

    /// Serialize a runtime value of type `ty` into its canonical (BCS) byte representation.
    fn serialize_value(vm: &Self::Vm<'_>, value: &Self::Value, ty: &TypeTag) -> VMResult<Vec<u8>>;
}

/// The verification facet of a Move subsystem: the base bytecode verifier.
///
/// The full API's second facet -- the abstract-interpretation framework contract
/// (`AbstractDomain`/`TransferFunctions`/`analyze_function`) used by sui-verifier's
/// `id_leak_verifier` -- is elided in this mock.
pub trait MoveVerifierHarness {
    /// Metered base verification of a single module. `CompiledModule`, the config, and the meter
    /// all come from shared (un-versioned) crates; only the verifier implementation behind this
    /// call is subsystem-versioned.
    fn verify_module(
        module: &CompiledModule,
        config: &VerifierConfig,
        meter: &mut (impl Meter + ?Sized),
    ) -> VMResult<()>;
}
