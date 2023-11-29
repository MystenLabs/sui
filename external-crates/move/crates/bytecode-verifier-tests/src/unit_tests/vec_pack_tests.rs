// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::unit_tests::production_config;
use move_binary_format::file_format::{
    empty_module, Bytecode, CodeUnit, FunctionDefinition, FunctionHandle, FunctionHandleIndex,
    IdentifierIndex, ModuleHandleIndex, Signature, SignatureIndex, SignatureToken, Visibility,
};
use move_bytecode_verifier::meter::DummyMeter;
use move_core_types::{identifier::Identifier, vm_status::StatusCode};

fn vec_sig(len: usize) -> SignatureToken {
    if len > 0 {
        SignatureToken::Vector(Box::new(vec_sig(len - 1)))
    } else {
        SignatureToken::U8
    }
}

#[test]
fn test_vec_pack() {
    let mut m = empty_module();

    let sig = SignatureIndex(m.signatures.len() as u16);
    m.signatures.push(Signature(vec![vec_sig(255)]));

    m.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Visibility::Private,
        is_entry: false,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![],
            jump_tables: vec![],
        }),
    });

    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(m.identifiers.len() as u16),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    m.identifiers
        .push(Identifier::new("foo".to_string()).unwrap());

    const COUNT: usize = 3000;

    m.function_defs[0].code.as_mut().unwrap().code =
        std::iter::once(&[Bytecode::VecPack(sig, 0)][..])
            .chain(
                std::iter::repeat(
                    &[Bytecode::VecUnpack(sig, 1024), Bytecode::VecPack(sig, 1024)][..],
                )
                .take(COUNT),
            )
            .chain(std::iter::once(&[Bytecode::Pop, Bytecode::Ret][..]))
            .flatten()
            .cloned()
            .collect();

    let res = move_bytecode_verifier::verify_module_with_config_for_test(
        "test_vec_pack",
        &production_config(),
        &m,
        &mut DummyMeter,
    )
    .unwrap_err();
    assert_eq!(res.major_status(), StatusCode::VALUE_STACK_PUSH_OVERFLOW);
}
