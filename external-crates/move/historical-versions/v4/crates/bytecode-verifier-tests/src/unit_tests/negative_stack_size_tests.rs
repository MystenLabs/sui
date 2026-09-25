// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::support::dummy_procedure_module;
use move_binary_format::file_format::Bytecode;
use move_bytecode_verifier::ability_cache::AbilityCache;
use move_bytecode_verifier::code_unit_verifier;
use move_bytecode_verifier_meter::dummy::DummyMeter;
use move_core_types::vm_status::StatusCode;

#[test]
fn one_pop_no_push() {
    let module = &dummy_procedure_module(vec![Bytecode::Pop, Bytecode::Ret]);
    let ability_cache = &mut AbilityCache::new(module);
    let result = code_unit_verifier::verify_module(
        &Default::default(),
        module,
        ability_cache,
        &mut DummyMeter,
    );
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::NEGATIVE_STACK_SIZE_WITHIN_BLOCK
    );
}

#[test]
fn one_pop_one_push() {
    // Height: 0 + (-1 + 1) = 0 would have passed original usage verifier
    let module = &dummy_procedure_module(vec![Bytecode::ReadRef, Bytecode::Ret]);
    let ability_cache = &mut AbilityCache::new(module);
    let result = code_unit_verifier::verify_module(
        &Default::default(),
        module,
        ability_cache,
        &mut DummyMeter,
    );
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::NEGATIVE_STACK_SIZE_WITHIN_BLOCK
    );
}

#[test]
fn two_pop_one_push() {
    // Height: 0 + 1 + (-2 + 1) = 0 would have passed original usage verifier
    let module = &dummy_procedure_module(vec![Bytecode::LdU64(0), Bytecode::Add, Bytecode::Ret]);
    let ability_cache = &mut AbilityCache::new(module);
    let result = code_unit_verifier::verify_module(
        &Default::default(),
        module,
        ability_cache,
        &mut DummyMeter,
    );
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::NEGATIVE_STACK_SIZE_WITHIN_BLOCK
    );
}

#[test]
fn two_pop_no_push() {
    let module = &dummy_procedure_module(vec![Bytecode::WriteRef, Bytecode::Ret]);
    let ability_cache = &mut AbilityCache::new(module);
    let result = code_unit_verifier::verify_module(
        &Default::default(),
        module,
        ability_cache,
        &mut DummyMeter,
    );
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::NEGATIVE_STACK_SIZE_WITHIN_BLOCK
    );
}
