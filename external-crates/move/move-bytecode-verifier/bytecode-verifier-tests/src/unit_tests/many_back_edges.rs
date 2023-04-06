// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::unit_tests::production_config;
use move_binary_format::file_format::{
    empty_module, Bytecode, CodeUnit, FunctionDefinition, FunctionHandle, FunctionHandleIndex,
    IdentifierIndex, ModuleHandleIndex, Signature, SignatureIndex, SignatureToken,
    Visibility::Public,
};
use move_core_types::{identifier::Identifier, vm_status::StatusCode};

const MAX_BASIC_BLOCKS: u16 = 1024;
const MAX_LOCALS: u8 = 255;

const NUM_FUNCTIONS: u16 = 16;

#[test]
fn many_backedges() {
    let mut m = empty_module();

    // signature of locals in f1..f<NUM_FUNCTIONS>
    m.signatures.push(Signature(
        std::iter::repeat(SignatureToken::U8)
            .take(MAX_LOCALS as usize)
            .collect(),
    ));

    // create returns_bool_and_u64
    m.signatures
        .push(Signature(vec![SignatureToken::Bool, SignatureToken::U8]));
    m.identifiers
        .push(Identifier::new("returns_bool_and_u64").unwrap());
    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(1),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(2),
        type_parameters: vec![],
    });
    m.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Public,
        is_entry: false,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![Bytecode::LdTrue, Bytecode::LdU8(0), Bytecode::Ret],
        }),
    });

    // create other functions
    for i in 1..(NUM_FUNCTIONS + 1) {
        m.identifiers
            .push(Identifier::new(format!("f{}", i)).unwrap());
        m.function_handles.push(FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(i + 1), // the +1 accounts for returns_bool_and_u64
            parameters: SignatureIndex(0),
            return_: SignatureIndex(0),
            type_parameters: vec![],
        });
        m.function_defs.push(FunctionDefinition {
            function: FunctionHandleIndex(i),
            visibility: Public,
            is_entry: false,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(1),
                code: vec![],
            }),
        });

        let code = &mut m.function_defs[i as usize].code.as_mut().unwrap().code;

        for _ in 0..(MAX_BASIC_BLOCKS - MAX_LOCALS as u16 - 2) {
            code.push(Bytecode::LdTrue);
            code.push(Bytecode::BrTrue(0));
        }
        for i in 0..MAX_LOCALS {
            code.push(Bytecode::Call(FunctionHandleIndex(0))); // calls returns_bool_and_u64
            code.push(Bytecode::StLoc(i as u8)); // i'th local is now available for the first time
            code.push(Bytecode::BrTrue(0));
        }
        code.push(Bytecode::Ret);
    }

    let result = move_bytecode_verifier::verify_module_with_config_for_test(
        "many_backedges",
        &production_config(),
        &m,
    );
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::CONSTRAINT_NOT_SATISFIED
    );
}
