// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Type-safety rules for the signed-integer instructions: `Neg` is signed-only and
//! type-preserving, `LdI*` pushes its exact width, and `CastI*` accepts any integer
//! operand.

use crate::support::dummy_procedure_module;
use move_binary_format::file_format::{
    Bytecode, CompiledModule, Signature, SignatureIndex, SignatureToken,
};
use move_bytecode_verifier::{ability_cache::AbilityCache, code_unit_verifier};
use move_bytecode_verifier_meter::dummy::DummyMeter;
use move_core_types::{i256::I256, u256::U256, vm_status::StatusCode};

fn module_with_locals(locals: Vec<SignatureToken>, code: Vec<Bytecode>) -> CompiledModule {
    let mut module = dummy_procedure_module(code);
    module.signatures.push(Signature(locals));
    module.function_defs[0].code.as_mut().unwrap().locals = SignatureIndex(1);
    module
}

fn verify(module: &CompiledModule) -> Result<(), StatusCode> {
    let ability_cache = &mut AbilityCache::new(module);
    code_unit_verifier::verify_module(&Default::default(), module, ability_cache, &mut DummyMeter)
        .map_err(|e| e.major_status())
}

fn signed_loads() -> Vec<(Bytecode, SignatureToken)> {
    vec![
        (Bytecode::LdI8(1), SignatureToken::I8),
        (Bytecode::LdI16(1), SignatureToken::I16),
        (Bytecode::LdI32(1), SignatureToken::I32),
        (Bytecode::LdI64(1), SignatureToken::I64),
        (Bytecode::LdI128(Box::new(1)), SignatureToken::I128),
        (
            Bytecode::LdI256(Box::new(I256::from(1i8))),
            SignatureToken::I256,
        ),
    ]
}

#[test]
fn ld_signed_pushes_exact_width() {
    // Storing into a local of the same width pins the pushed type exactly.
    for (ld, ty) in signed_loads() {
        let module = module_with_locals(
            vec![ty.clone()],
            vec![ld, Bytecode::StLoc(0), Bytecode::Ret],
        );
        assert!(verify(&module).is_ok(), "LdI* must push {ty:?}");
    }
    let module = module_with_locals(
        vec![SignatureToken::I16],
        vec![Bytecode::LdI8(1), Bytecode::StLoc(0), Bytecode::Ret],
    );
    assert_eq!(verify(&module), Err(StatusCode::STLOC_TYPE_MISMATCH_ERROR));
}

#[test]
fn neg_preserves_operand_type() {
    for (ld, ty) in signed_loads() {
        let module = module_with_locals(
            vec![ty.clone()],
            vec![ld, Bytecode::Neg, Bytecode::StLoc(0), Bytecode::Ret],
        );
        assert!(verify(&module).is_ok(), "Neg on {ty:?} must push {ty:?}");
    }
}

#[test]
fn neg_rejects_non_signed_operands() {
    let unsigned_loads = vec![
        Bytecode::LdU8(1),
        Bytecode::LdU16(1),
        Bytecode::LdU32(1),
        Bytecode::LdU64(1),
        Bytecode::LdU128(Box::new(1)),
        Bytecode::LdU256(Box::new(U256::from(1u8))),
        Bytecode::LdTrue,
    ];
    for ld in unsigned_loads {
        let module = dummy_procedure_module(vec![
            ld.clone(),
            Bytecode::Neg,
            Bytecode::Pop,
            Bytecode::Ret,
        ]);
        assert_eq!(
            verify(&module),
            Err(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR),
            "Neg must reject {ld:?}"
        );
    }
    let module = module_with_locals(
        vec![SignatureToken::U64],
        vec![
            Bytecode::LdU64(1),
            Bytecode::StLoc(0),
            Bytecode::ImmBorrowLoc(0),
            Bytecode::Neg,
            Bytecode::Pop,
            Bytecode::Ret,
        ],
    );
    assert_eq!(
        verify(&module),
        Err(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR)
    );
}

#[test]
fn cast_signed_accepts_any_integer_operand() {
    let casts = [
        (Bytecode::CastI8, SignatureToken::I8),
        (Bytecode::CastI16, SignatureToken::I16),
        (Bytecode::CastI32, SignatureToken::I32),
        (Bytecode::CastI64, SignatureToken::I64),
        (Bytecode::CastI128, SignatureToken::I128),
        (Bytecode::CastI256, SignatureToken::I256),
    ];
    for (cast, target) in casts {
        for operand in [Bytecode::LdU64(1), Bytecode::LdI64(1)] {
            let module = module_with_locals(
                vec![target.clone()],
                vec![
                    operand.clone(),
                    cast.clone(),
                    Bytecode::StLoc(0),
                    Bytecode::Ret,
                ],
            );
            assert!(
                verify(&module).is_ok(),
                "{cast:?} must accept {operand:?} and push {target:?}"
            );
        }
    }
    // Unsigned casts accept signed operands too; the range check happens at runtime.
    let module = dummy_procedure_module(vec![
        Bytecode::LdI64(1),
        Bytecode::CastU8,
        Bytecode::Pop,
        Bytecode::Ret,
    ]);
    assert!(verify(&module).is_ok());
}

#[test]
fn cast_signed_rejects_non_integer_operands() {
    let module = dummy_procedure_module(vec![
        Bytecode::LdTrue,
        Bytecode::CastI8,
        Bytecode::Pop,
        Bytecode::Ret,
    ]);
    assert_eq!(
        verify(&module),
        Err(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR)
    );
    let module = module_with_locals(
        vec![SignatureToken::U64],
        vec![
            Bytecode::LdU64(1),
            Bytecode::StLoc(0),
            Bytecode::ImmBorrowLoc(0),
            Bytecode::CastI32,
            Bytecode::Pop,
            Bytecode::Ret,
        ],
    );
    assert_eq!(
        verify(&module),
        Err(StatusCode::INTEGER_OP_TYPE_MISMATCH_ERROR)
    );
}
