// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for the tx_context_restrictions_verifier rejection path, which
//! cannot be exercised through transactional tests: publishes there never land
//! on a system-package address, so the pass always exempts them. Here we
//! hand-construct modules with a chosen self-address and call the pass
//! directly; only the signature tables need to be well-formed.

use move_binary_format::{
    CompiledModule,
    file_format::{
        AbilitySet, AddressIdentifierIndex, DatatypeHandle, DatatypeHandleIndex,
        FunctionDefinition, FunctionHandle, FunctionHandleIndex, IdentifierIndex, ModuleHandle,
        ModuleHandleIndex, Signature, SignatureIndex, SignatureToken, Visibility, empty_module,
    },
};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_vm_config::verifier::VerifierConfig;
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_verifier_latest::tx_context_restrictions_verifier::verify_module;

/// A module at `self_addr` with a single public function `f` of the given
/// signature. `Datatype(0)` in the signature tokens is `0x2::tx_context::TxContext`.
fn module_with_function(
    self_addr: AccountAddress,
    parameters: Vec<SignatureToken>,
    returns: Vec<SignatureToken>,
) -> CompiledModule {
    let mut m = empty_module();
    m.address_identifiers[0] = self_addr;
    m.address_identifiers.push(SUI_FRAMEWORK_ADDRESS);
    m.identifiers.push(Identifier::new("tx_context").unwrap());
    m.module_handles.push(ModuleHandle {
        address: AddressIdentifierIndex(1),
        name: IdentifierIndex((m.identifiers.len() - 1) as u16),
    });
    m.identifiers.push(Identifier::new("TxContext").unwrap());
    m.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(1),
        name: IdentifierIndex((m.identifiers.len() - 1) as u16),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    });
    m.signatures.push(Signature(parameters));
    m.signatures.push(Signature(returns));
    m.identifiers.push(Identifier::new("f").unwrap());
    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex((m.identifiers.len() - 1) as u16),
        parameters: SignatureIndex(1),
        return_: SignatureIndex(2),
        type_parameters: vec![],
    });
    // `code: None` mirrors a native function; the pass covers those too and
    // never looks at code.
    m.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Visibility::Public,
        is_entry: false,
        acquires_global_resources: vec![],
        code: None,
    });
    m
}

fn mut_tx_context() -> SignatureToken {
    SignatureToken::MutableReference(Box::new(SignatureToken::Datatype(DatatypeHandleIndex(0))))
}

fn imm_tx_context() -> SignatureToken {
    SignatureToken::Reference(Box::new(SignatureToken::Datatype(DatatypeHandleIndex(0))))
}

fn mut_u64() -> SignatureToken {
    SignatureToken::MutableReference(Box::new(SignatureToken::U64))
}

fn config(enabled: bool) -> VerifierConfig {
    VerifierConfig {
        framework_tx_context_mut_restrictions: enabled,
        ..VerifierConfig::default()
    }
}

const USER_ADDRESS: AccountAddress = AccountAddress::new([0xCA; AccountAddress::LENGTH]);

#[test]
fn rejects_mut_return_without_root_on_system_package() {
    let m = module_with_function(
        SUI_FRAMEWORK_ADDRESS,
        vec![mut_tx_context()],
        vec![mut_u64()],
    );
    let err = verify_module(&m, &config(true)).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("::f"), "unexpected error: {msg}");
    assert!(
        msg.contains("cannot serve as the mutable root"),
        "unexpected error: {msg}"
    );
}

#[test]
fn rejects_mut_tx_context_identity_on_system_package() {
    let m = module_with_function(
        SUI_FRAMEWORK_ADDRESS,
        vec![mut_tx_context()],
        vec![mut_tx_context()],
    );
    assert!(verify_module(&m, &config(true)).is_err());
}

#[test]
fn accepts_mut_return_with_non_tx_context_root() {
    let m = module_with_function(
        SUI_FRAMEWORK_ADDRESS,
        vec![mut_tx_context(), mut_u64()],
        vec![mut_u64()],
    );
    assert!(verify_module(&m, &config(true)).is_ok());
}

#[test]
fn accepts_without_mut_return() {
    // Immutable returns and value returns are out of scope even with
    // `&mut TxContext` in the parameters.
    let m = module_with_function(
        SUI_FRAMEWORK_ADDRESS,
        vec![mut_tx_context()],
        vec![
            SignatureToken::Reference(Box::new(SignatureToken::U64)),
            SignatureToken::U64,
        ],
    );
    assert!(verify_module(&m, &config(true)).is_ok());
}

#[test]
fn accepts_without_mut_tx_context_param() {
    // `&TxContext` is not a mutable root, so the function is out of scope.
    let m = module_with_function(
        SUI_FRAMEWORK_ADDRESS,
        vec![imm_tx_context()],
        vec![mut_u64()],
    );
    assert!(verify_module(&m, &config(true)).is_ok());
}

#[test]
fn exempts_user_packages() {
    let m = module_with_function(USER_ADDRESS, vec![mut_tx_context()], vec![mut_u64()]);
    assert!(verify_module(&m, &config(true)).is_ok());
}

#[test]
fn no_ops_with_flag_off() {
    let m = module_with_function(
        SUI_FRAMEWORK_ADDRESS,
        vec![mut_tx_context()],
        vec![mut_u64()],
    );
    assert!(verify_module(&m, &config(false)).is_ok());
}
