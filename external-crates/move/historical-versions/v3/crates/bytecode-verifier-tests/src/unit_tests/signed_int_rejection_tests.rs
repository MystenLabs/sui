// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Signed integers postdate this execution version: any signed signature token or
//! opcode must fail verification.

use crate::support::dummy_procedure_module;
use move_binary_format::{
    errors::VMResult,
    file_format::{Bytecode, Signature, SignatureIndex, SignatureToken, basic_test_module},
};
use move_bytecode_verifier::{
    SignatureChecker, ability_cache::AbilityCache, verify_module_unmetered,
};
use move_bytecode_verifier_meter::dummy::DummyMeter;
use move_core_types::{i256::I256, vm_status::StatusCode};

fn assert_rejected(result: VMResult<()>, what: &str) {
    let err = result.expect_err("signed integer content must be rejected");
    assert_eq!(
        err.major_status(),
        StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
        "unexpected status for {what}"
    );
    let message = format!("{err:?}");
    assert!(
        message.contains("signed int"),
        "unexpected message for {what}: {message}"
    );
}

#[test]
fn signed_signature_token_rejected() {
    for ty in [
        SignatureToken::I8,
        SignatureToken::I16,
        SignatureToken::I32,
        SignatureToken::I64,
        SignatureToken::I128,
        SignatureToken::I256,
    ] {
        let mut m = basic_test_module();
        let signature_idx = SignatureIndex(m.signatures.len() as u16);
        m.signatures.push(Signature(vec![ty.clone()]));
        m.function_defs[0].code.as_mut().unwrap().locals = signature_idx;
        let ability_cache = &mut AbilityCache::new(&m);
        assert_rejected(
            SignatureChecker::verify_module(&m, ability_cache, &mut DummyMeter),
            &format!("signature checker {ty:?}"),
        );
        assert_rejected(
            verify_module_unmetered(&m),
            &format!("full verification {ty:?}"),
        );
    }
}

#[test]
fn signed_opcode_rejected() {
    let programs = vec![
        vec![Bytecode::LdI8(1), Bytecode::Pop, Bytecode::Ret],
        vec![Bytecode::LdI16(1), Bytecode::Pop, Bytecode::Ret],
        vec![Bytecode::LdI32(1), Bytecode::Pop, Bytecode::Ret],
        vec![Bytecode::LdI64(1), Bytecode::Pop, Bytecode::Ret],
        vec![Bytecode::LdI128(Box::new(1)), Bytecode::Pop, Bytecode::Ret],
        vec![
            Bytecode::LdI256(Box::new(I256::from(1i8))),
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        vec![
            Bytecode::LdU8(1),
            Bytecode::CastI8,
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        vec![
            Bytecode::LdU8(1),
            Bytecode::CastI16,
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        vec![
            Bytecode::LdU8(1),
            Bytecode::CastI32,
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        vec![
            Bytecode::LdU8(1),
            Bytecode::CastI64,
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        vec![
            Bytecode::LdU8(1),
            Bytecode::CastI128,
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        vec![
            Bytecode::LdU8(1),
            Bytecode::CastI256,
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        vec![
            Bytecode::LdU8(1),
            Bytecode::Neg,
            Bytecode::Pop,
            Bytecode::Ret,
        ],
    ];
    for code in programs {
        let module = dummy_procedure_module(code.clone());
        assert_rejected(
            verify_module_unmetered(&module),
            &format!(
                "opcode {:?}",
                code[..code.len() - 2].iter().collect::<Vec<_>>()
            ),
        );
    }
}
