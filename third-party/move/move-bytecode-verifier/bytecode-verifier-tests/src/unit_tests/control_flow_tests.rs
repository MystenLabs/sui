// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::dummy_procedure_module;
use move_binary_format::{
    access::ModuleAccess,
    errors::PartialVMResult,
    file_format::{Bytecode, CompiledModule, FunctionDefinitionIndex, TableIndex},
};
use move_bytecode_verifier::{control_flow, meter::DummyMeter, VerifierConfig};
use move_core_types::vm_status::StatusCode;

fn verify_module(verifier_config: &VerifierConfig, module: &CompiledModule) -> PartialVMResult<()> {
    for (idx, function_definition) in module
        .function_defs()
        .iter()
        .enumerate()
        .filter(|(_, def)| !def.is_native())
    {
        let current_function = FunctionDefinitionIndex(idx as TableIndex);
        let code = function_definition
            .code
            .as_ref()
            .expect("unexpected native function");

        control_flow::verify_function(
            verifier_config,
            module,
            current_function,
            function_definition,
            code,
            &mut DummyMeter,
        )?;
    }
    Ok(())
}

//**************************************************************************************************
// Simple cases -  Copied from code unit verifier
//**************************************************************************************************

#[test]
fn empty_bytecode() {
    let module = dummy_procedure_module(vec![]);
    let result = verify_module(&Default::default(), &module);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::EMPTY_CODE_UNIT,
    );
}

#[test]
fn empty_bytecode_v5() {
    let mut module = dummy_procedure_module(vec![]);
    module.version = 5;

    let result = verify_module(&Default::default(), &module);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::EMPTY_CODE_UNIT,
    );
}

#[test]
fn invalid_fallthrough_br_true() {
    let module = dummy_procedure_module(vec![Bytecode::LdFalse, Bytecode::BrTrue(1)]);
    let result = verify_module(&Default::default(), &module);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::INVALID_FALL_THROUGH
    );
}

#[test]
fn invalid_fallthrough_br_false() {
    let module = dummy_procedure_module(vec![Bytecode::LdTrue, Bytecode::BrFalse(1)]);
    let result = verify_module(&Default::default(), &module);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::INVALID_FALL_THROUGH
    );
}

// all non-branch instructions should trigger invalid fallthrough; just check one of them
#[test]
fn invalid_fallthrough_non_branch() {
    let module = dummy_procedure_module(vec![Bytecode::LdTrue, Bytecode::Pop]);
    let result = verify_module(&Default::default(), &module);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::INVALID_FALL_THROUGH
    );
}

#[test]
fn valid_fallthrough_branch() {
    let module = dummy_procedure_module(vec![Bytecode::Branch(0)]);
    let result = verify_module(&Default::default(), &module);
    assert!(result.is_ok());
}

#[test]
fn valid_fallthrough_ret() {
    let module = dummy_procedure_module(vec![Bytecode::Ret]);
    let result = verify_module(&Default::default(), &module);
    assert!(result.is_ok());
}

#[test]
fn valid_fallthrough_abort() {
    let module = dummy_procedure_module(vec![Bytecode::LdU64(7), Bytecode::Abort]);
    let result = verify_module(&Default::default(), &module);
    assert!(result.is_ok());
}

#[test]
fn nested_loops_max_depth() {
    let module = dummy_procedure_module(vec![
        Bytecode::LdFalse,
        Bytecode::LdFalse,
        Bytecode::BrFalse(1),
        Bytecode::BrFalse(0),
        Bytecode::Ret,
    ]);
    let result = verify_module(
        &VerifierConfig {
            max_loop_depth: Some(2),
            ..VerifierConfig::default()
        },
        &module,
    );
    assert!(result.is_ok());
}

#[test]
fn nested_loops_exceed_max_depth() {
    let module = dummy_procedure_module(vec![
        Bytecode::LdFalse,
        Bytecode::LdFalse,
        Bytecode::LdFalse,
        Bytecode::BrFalse(2),
        Bytecode::BrFalse(1),
        Bytecode::BrFalse(0),
        Bytecode::Ret,
    ]);
    let result = verify_module(
        &VerifierConfig {
            max_loop_depth: Some(2),
            ..VerifierConfig::default()
        },
        &module,
    );
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::LOOP_MAX_DEPTH_REACHED
    );
}

#[test]
fn non_loop_backward_jump() {
    let module = dummy_procedure_module(vec![
        Bytecode::Branch(2),
        Bytecode::Ret,
        Bytecode::Branch(1),
    ]);
    let result = verify_module(&Default::default(), &module);
    assert!(result.is_ok());
}

#[test]
fn non_loop_backward_jump_v5() {
    let mut module = dummy_procedure_module(vec![
        Bytecode::Branch(2),
        Bytecode::Ret,
        Bytecode::Branch(1),
    ]);

    module.version = 5;
    let result = verify_module(&Default::default(), &module);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::INVALID_LOOP_SPLIT,
    );
}

#[test]
fn irreducible_control_flow_graph() {
    let module = dummy_procedure_module(vec![
        Bytecode::LdTrue,
        Bytecode::BrTrue(3),
        Bytecode::Nop,
        Bytecode::LdFalse,
        Bytecode::BrFalse(2),
        Bytecode::Ret,
    ]);
    let result = verify_module(&Default::default(), &module);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::INVALID_LOOP_SPLIT,
    );
}

#[test]
fn nested_loop_break() {
    let module = dummy_procedure_module(vec![
        Bytecode::LdFalse,
        Bytecode::LdFalse,
        Bytecode::LdFalse,
        Bytecode::Branch(7),
        Bytecode::BrFalse(2),
        Bytecode::BrFalse(1),
        Bytecode::BrFalse(0),
        Bytecode::Ret,
    ]);
    let result = verify_module(&Default::default(), &module);
    assert!(result.is_ok());
}

#[test]
fn nested_loop_break_v5() {
    let mut module = dummy_procedure_module(vec![
        Bytecode::LdFalse,
        Bytecode::LdFalse,
        Bytecode::LdFalse,
        Bytecode::Branch(7),
        Bytecode::BrFalse(2),
        Bytecode::BrFalse(1),
        Bytecode::BrFalse(0),
        Bytecode::Ret,
    ]);

    module.version = 5;
    let result = verify_module(&Default::default(), &module);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::INVALID_LOOP_BREAK,
    );
}
