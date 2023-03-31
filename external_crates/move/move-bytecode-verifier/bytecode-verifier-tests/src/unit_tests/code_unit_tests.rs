// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::dummy_procedure_module;
use move_binary_format::file_format::Bytecode;
use move_bytecode_verifier::{CodeUnitVerifier, VerifierConfig};
use move_core_types::vm_status::StatusCode;

#[test]
fn invalid_fallthrough_br_true() {
    let module = dummy_procedure_module(vec![Bytecode::LdFalse, Bytecode::BrTrue(1)]);
    let result = CodeUnitVerifier::verify_module(&Default::default(), &module);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::INVALID_FALL_THROUGH
    );
}

#[test]
fn invalid_fallthrough_br_false() {
    let module = dummy_procedure_module(vec![Bytecode::LdTrue, Bytecode::BrFalse(1)]);
    let result = CodeUnitVerifier::verify_module(&Default::default(), &module);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::INVALID_FALL_THROUGH
    );
}

// all non-branch instructions should trigger invalid fallthrough; just check one of them
#[test]
fn invalid_fallthrough_non_branch() {
    let module = dummy_procedure_module(vec![Bytecode::LdTrue, Bytecode::Pop]);
    let result = CodeUnitVerifier::verify_module(&Default::default(), &module);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::INVALID_FALL_THROUGH
    );
}

#[test]
fn valid_fallthrough_branch() {
    let module = dummy_procedure_module(vec![Bytecode::Branch(0)]);
    let result = CodeUnitVerifier::verify_module(&Default::default(), &module);
    assert!(result.is_ok());
}

#[test]
fn valid_fallthrough_ret() {
    let module = dummy_procedure_module(vec![Bytecode::Ret]);
    let result = CodeUnitVerifier::verify_module(&Default::default(), &module);
    assert!(result.is_ok());
}

#[test]
fn valid_fallthrough_abort() {
    let module = dummy_procedure_module(vec![Bytecode::LdU64(7), Bytecode::Abort]);
    let result = CodeUnitVerifier::verify_module(&Default::default(), &module);
    assert!(result.is_ok());
}

#[test]
fn test_max_number_of_bytecode() {
    let mut nops = vec![];
    for _ in 0..u16::MAX - 1 {
        nops.push(Bytecode::Nop);
    }
    nops.push(Bytecode::Ret);
    let module = dummy_procedure_module(nops);

    let result = CodeUnitVerifier::verify_module(&VerifierConfig::unbounded(), &module);
    assert!(result.is_ok());
}

#[test]
fn test_max_basic_blocks() {
    let mut code = (0..17)
        .map(|idx| Bytecode::Branch(idx + 1))
        .collect::<Vec<_>>();
    code.push(Bytecode::Ret);
    let module = dummy_procedure_module(code);

    let result = CodeUnitVerifier::verify_module(
        &VerifierConfig {
            max_basic_blocks: Some(16),
            ..Default::default()
        },
        &module,
    );
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::TOO_MANY_BASIC_BLOCKS
    );
}
