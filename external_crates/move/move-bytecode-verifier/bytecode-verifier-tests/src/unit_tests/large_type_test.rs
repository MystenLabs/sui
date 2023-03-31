// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::unit_tests::production_config;
use move_binary_format::file_format::{
    empty_module, Bytecode, CodeUnit, FunctionDefinition, FunctionHandle, FunctionHandleIndex,
    IdentifierIndex, ModuleHandleIndex, Signature, SignatureIndex, SignatureToken,
    Visibility::Public,
};
use move_core_types::{identifier::Identifier, vm_status::StatusCode};

const NUM_LOCALS: u8 = 64;
const NUM_CALLS: u16 = 77;
const NUM_FUNCTIONS: u16 = 177;

fn get_nested_vec_type(len: usize) -> SignatureToken {
    let mut ret = SignatureToken::Bool;
    for _ in 0..len {
        ret = SignatureToken::Vector(Box::new(ret));
    }
    ret
}

#[test]
fn test_large_types() {
    // See also: github.com/aptos-labs/aptos-core/security/advisories/GHSA-37qw-jfpw-8899
    let mut m = empty_module();

    m.signatures.push(Signature(
        std::iter::repeat(SignatureToken::Reference(Box::new(get_nested_vec_type(64))))
            .take(NUM_LOCALS as usize)
            .collect(),
    ));

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
        is_entry: false,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![Bytecode::Call(FunctionHandleIndex(0)), Bytecode::Ret],
        }),
    });

    // returns_vecs
    m.identifiers.push(Identifier::new("returns_vecs").unwrap());
    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(1),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(1),
        type_parameters: vec![],
    });
    m.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(1),
        visibility: Public,
        is_entry: false,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![Bytecode::Call(FunctionHandleIndex(1)), Bytecode::Ret],
        }),
    });

    // takes_and_returns_vecs
    m.identifiers
        .push(Identifier::new("takes_and_returns_vecs").unwrap());
    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(2),
        parameters: SignatureIndex(1),
        return_: SignatureIndex(1),
        type_parameters: vec![],
    });
    m.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(2),
        visibility: Public,
        is_entry: false,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![Bytecode::Call(FunctionHandleIndex(1)), Bytecode::Ret],
        }),
    });

    // takes_vecs
    m.identifiers.push(Identifier::new("takes_vecs").unwrap());
    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(3),
        parameters: SignatureIndex(1),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    m.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(3),
        visibility: Public,
        is_entry: false,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![Bytecode::Ret],
        }),
    });

    // other fcts
    for i in 0..NUM_FUNCTIONS {
        m.identifiers
            .push(Identifier::new(format!("f{}", i)).unwrap());
        m.function_handles.push(FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(i + 4),
            parameters: SignatureIndex(0),
            return_: SignatureIndex(0),
            type_parameters: vec![],
        });
        m.function_defs.push(FunctionDefinition {
            function: FunctionHandleIndex(i + 4),
            visibility: Public,
            is_entry: false,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(0),
                code: vec![],
            }),
        });

        let code = &mut m.function_defs[i as usize + 4].code.as_mut().unwrap().code;
        code.clear();
        code.push(Bytecode::Call(FunctionHandleIndex(1)));
        for _ in 0..NUM_CALLS {
            code.push(Bytecode::Call(FunctionHandleIndex(2)));
        }
        code.push(Bytecode::Call(FunctionHandleIndex(3)));
        code.push(Bytecode::Ret);
    }

    let result = move_bytecode_verifier::verify_module_with_config_for_test(
        "test_large_types",
        &production_config(),
        &m,
    );
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::CONSTRAINT_NOT_SATISFIED,
    );
}
