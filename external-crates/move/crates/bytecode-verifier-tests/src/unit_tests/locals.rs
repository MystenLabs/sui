// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::unit_tests::production_config;
use move_binary_format::file_format::{
    empty_module, Bytecode, CodeUnit, FunctionDefinition, FunctionHandle, FunctionHandleIndex,
    IdentifierIndex, ModuleHandleIndex, Signature, SignatureIndex, SignatureToken,
    Visibility::Public,
};
use move_bytecode_verifier::meter::BoundMeter;
use move_core_types::{identifier::Identifier, vm_status::StatusCode};

#[test]
fn test_locals() {
    // See also: github.com/aptos-labs/aptos-core/security/advisories/GHSA-jjqw-f9pc-525j
    let mut m = empty_module();

    const MAX_BASIC_BLOCKS: u16 = 1024;
    const MAX_LOCALS: u8 = 255;
    const NUM_FUNCTIONS: u16 = 16;

    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(0),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });

    m.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Public,
        is_entry: true,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![Bytecode::Ret],
            jump_tables: vec![],
        }),
    });

    // signature of locals in f1..f<NUM_FUNCTIONS>
    m.signatures.push(Signature(
        std::iter::repeat(SignatureToken::U8)
            .take(MAX_LOCALS as usize)
            .collect(),
    ));

    m.identifiers.push(Identifier::new("pwn").unwrap());

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
        function: FunctionHandleIndex(1),
        visibility: Public,
        is_entry: false,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![Bytecode::LdTrue, Bytecode::LdU8(0), Bytecode::Ret],
            jump_tables: vec![],
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
            function: FunctionHandleIndex(i + 1),
            visibility: Public,
            is_entry: false,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(1),
                code: vec![],
                jump_tables: vec![],
            }),
        });

        let code = &mut m.function_defs[i as usize + 1].code.as_mut().unwrap().code;

        for _ in 0..(MAX_BASIC_BLOCKS / 2 - MAX_LOCALS as u16 - 3) {
            code.push(Bytecode::LdTrue);
            code.push(Bytecode::BrTrue((code.len() + 2) as u16));
            code.push(Bytecode::Ret);
            code.push(Bytecode::LdTrue);
            code.push(Bytecode::BrTrue(0));
        }
        for i in 0..MAX_LOCALS {
            code.push(Bytecode::Call(FunctionHandleIndex(1))); // calls returns_bool_and_u64
            code.push(Bytecode::StLoc(i as u8)); // i'th local is now available for the first time
            code.push(Bytecode::BrTrue(0));
        }
        code.push(Bytecode::Ret);
    }

    let config = production_config();
    let mut meter = BoundMeter::new(&config);
    let result = move_bytecode_verifier::verify_module_with_config_for_test(
        "test_locals",
        &config,
        &m,
        &mut meter,
    );
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::CONSTRAINT_NOT_SATISFIED
    );
}
