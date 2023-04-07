// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
use fail::FailScenario;
use move_binary_format::file_format::empty_module;
use move_bytecode_verifier::VerifierConfig;
use move_core_types::{
    state::{self, VMState},
    vm_status::StatusCode,
};
use std::panic::{self, PanicInfo};

#[test]
fn test_unwind() {
    let scenario = FailScenario::setup();
    fail::cfg("verifier-failpoint-panic", "panic").unwrap();

    panic::set_hook(Box::new(move |_: &PanicInfo<'_>| {
        assert_eq!(state::get_state(), VMState::VERIFIER);
    }));

    let m = empty_module();
    let res = move_bytecode_verifier::verify_module_with_config(&VerifierConfig::unbounded(), &m)
        .unwrap_err();
    assert_eq!(res.major_status(), StatusCode::VERIFIER_INVARIANT_VIOLATION);
    scenario.teardown();
}
