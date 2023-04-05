// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::unit_tests::production_config;
use move_binary_format::file_format::{
    empty_module, Bytecode, CodeUnit, FunctionDefinition, FunctionHandle, FunctionHandleIndex,
    IdentifierIndex, ModuleHandleIndex, Signature, SignatureIndex, SignatureToken,
    Visibility::Public,
};
use move_core_types::{identifier::Identifier, vm_status::StatusCode};

#[test]
fn test_bicliques() {
    // See also: github.com/aptos-labs/aptos-core/security/advisories/GHSA-xm6p-ffcq-5p2v
    const NUM_LOCALS: u8 = 128;
    const NUM_CALLS: u16 = 76;
    const NUM_FUNCTIONS: u16 = 1;

    let mut m = empty_module();

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

    // create take_and_return_references
    m.signatures.push(Signature(
        std::iter::repeat(SignatureToken::Reference(Box::new(SignatureToken::U64)))
            .take(NUM_LOCALS as usize)
            .collect(),
    ));
    m.identifiers
        .push(Identifier::new("take_and_return_references").unwrap());
    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(1),
        parameters: SignatureIndex(1),
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
            code: vec![],
        }),
    });
    let code = &mut m.function_defs[1].code.as_mut().unwrap().code;
    for i in 0..NUM_LOCALS {
        code.push(Bytecode::MoveLoc(i));
    }
    code.push(Bytecode::Ret);

    // create swallow_references
    m.identifiers
        .push(Identifier::new("swallow_references").unwrap());
    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(2),
        parameters: SignatureIndex(1),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    m.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(2),
        visibility: Public,
        is_entry: false,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![Bytecode::Ret],
        }),
    });

    // create other functions
    for i in 1..(NUM_FUNCTIONS + 1) {
        m.identifiers
            .push(Identifier::new(format!("f{}", i)).unwrap());
        m.function_handles.push(FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(i + 2),
            parameters: SignatureIndex(1),
            return_: SignatureIndex(0),
            type_parameters: vec![],
        });
        m.function_defs.push(FunctionDefinition {
            function: FunctionHandleIndex(i + 2),
            visibility: Public,
            is_entry: false,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(0),
                code: vec![],
            }),
        });
        let code = &mut m.function_defs[i as usize + 2].code.as_mut().unwrap().code;
        for j in 0..NUM_LOCALS {
            code.push(Bytecode::CopyLoc(j));
        }
        for _ in 0..NUM_CALLS {
            code.push(Bytecode::Call(FunctionHandleIndex(1)));
        }
        code.push(Bytecode::Call(FunctionHandleIndex(2)));
        code.push(Bytecode::Ret);
    }

    let result = move_bytecode_verifier::verify_module_with_config_for_test(
        "test_bicliques",
        &production_config(),
        &m,
    );
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::CONSTRAINT_NOT_SATISFIED
    );
}

#[test]
fn test_merge_state_large_graph() {
    // See also: github.com/aptos-labs/aptos-core/security/advisories/GHSA-g8v8-fw4c-8h82
    const N: u8 = 127;
    const NUM_NOP_BLOCKS: u16 = 950;
    const NUM_FUNCTIONS: u16 = 18;

    let mut m = empty_module();

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

    m.signatures.push(Signature(
        std::iter::repeat(SignatureToken::Reference(Box::new(SignatureToken::U8)))
            .take(N as usize)
            .collect(),
    ));

    m.identifiers.push(Identifier::new("return_refs").unwrap());
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

    m.identifiers
        .push(Identifier::new("take_and_return_refs").unwrap());
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

    for i in 0..NUM_FUNCTIONS {
        m.identifiers
            .push(Identifier::new(format!("f{}", i)).unwrap());
        m.function_handles.push(FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(i + 3),
            parameters: SignatureIndex(1),
            return_: SignatureIndex(0),
            type_parameters: vec![],
        });
        m.function_defs.push(FunctionDefinition {
            function: FunctionHandleIndex(i + 3),
            visibility: Public,
            is_entry: false,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(1),
                code: vec![],
            }),
        });
        let code = &mut m.function_defs[i as usize + 3].code.as_mut().unwrap().code;
        for j in 0..N {
            code.push(Bytecode::CopyLoc(j));
        }
        code.push(Bytecode::Call(FunctionHandleIndex(2)));
        for j in 0..N {
            code.push(Bytecode::StLoc(N + j));
        }
        for _ in 0..NUM_NOP_BLOCKS {
            code.push(Bytecode::LdTrue);
            code.push(Bytecode::BrTrue(0));
        }

        code.push(Bytecode::Ret);
    }

    let res = move_bytecode_verifier::verify_module_with_config_for_test(
        "test_merge_state_large_graph",
        &production_config(),
        &m,
    );
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::CONSTRAINT_NOT_SATISFIED
    );
}

#[test]
fn test_merge_state() {
    // See also: github.com/aptos-labs/aptos-core/security/advisories/GHSA-g8v8-fw4c-8h82
    const NUM_NOP_BLOCKS: u16 = 965;
    const NUM_LOCALS: u8 = 32;
    const NUM_FUNCTIONS: u16 = 21;

    let mut m = empty_module();

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

    m.signatures
        .push(Signature(vec![SignatureToken::Reference(Box::new(
            SignatureToken::U8,
        ))]));
    m.signatures.push(Signature(
        std::iter::repeat(SignatureToken::Reference(Box::new(SignatureToken::U8)))
            .take(NUM_LOCALS as usize - 1)
            .collect(),
    ));

    for i in 0..NUM_FUNCTIONS {
        m.identifiers
            .push(Identifier::new(format!("f{}", i)).unwrap());
        m.function_handles.push(FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(i + 1),
            parameters: SignatureIndex(1),
            return_: SignatureIndex(0),
            type_parameters: vec![],
        });
        m.function_defs.push(FunctionDefinition {
            function: FunctionHandleIndex(i + 1),
            visibility: Public,
            is_entry: false,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(2),
                code: vec![],
            }),
        });
        let code = &mut m.function_defs[i as usize + 1].code.as_mut().unwrap().code;
        // create reference id
        code.push(Bytecode::CopyLoc(0));
        code.push(Bytecode::StLoc(1));
        // create a path of length NUM_LOCALS - 1 in the borrow graph
        for j in 0..(NUM_LOCALS - 2) {
            // create Ref(new_id) and factor in empty-path edge id -> new_id
            code.push(Bytecode::CopyLoc(1));
            // can't leave those references on stack since basic blocks need to be stack-neutral
            code.push(Bytecode::StLoc(j + 2));
        }
        for _ in 0..NUM_NOP_BLOCKS {
            code.push(Bytecode::LdTrue);
            // create back edge to first block
            code.push(Bytecode::BrTrue(0));
        }

        code.push(Bytecode::Ret);
    }

    let res = move_bytecode_verifier::verify_module_with_config_for_test(
        "test_merge_state",
        &production_config(),
        &m,
    );
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::CONSTRAINT_NOT_SATISFIED
    );
}

#[test]
fn test_copyloc_pop() {
    // See also: github.com/aptos-labs/aptos-core/security/advisories/GHSA-2qvr-c9qp-wch7
    const NUM_COPYLOCS: u16 = 1880;
    const NUM_CHILDREN: u16 = 1020;
    const NUM_FUNCTIONS: u16 = 2;

    let mut m = empty_module();

    // parameters of f0, f1, ...
    m.signatures
        .push(Signature(vec![SignatureToken::Reference(Box::new(
            SignatureToken::Vector(Box::new(SignatureToken::U8)),
        ))]));
    // locals of f0, f1, ...
    m.signatures.push(Signature(vec![
        SignatureToken::Reference(Box::new(SignatureToken::Vector(Box::new(
            SignatureToken::U8,
        )))),
        SignatureToken::U8, // ignore this, it's just here because I don't want to fix indices and the TypeParameter after removing the collision
    ]));
    // for VecImmBorrow
    m.signatures.push(Signature(
        std::iter::repeat(SignatureToken::U8).take(1).collect(),
    ));
    m.signatures
        .push(Signature(vec![SignatureToken::TypeParameter(0)]));

    for i in 0..NUM_FUNCTIONS {
        m.identifiers
            .push(Identifier::new(format!("f{}", i)).unwrap());
        m.function_handles.push(FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(i),
            parameters: SignatureIndex(1),
            return_: SignatureIndex(0),
            type_parameters: vec![],
        });
        m.function_defs.push(FunctionDefinition {
            function: FunctionHandleIndex(i),
            visibility: Public,
            is_entry: false,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(2),
                code: vec![],
            }),
        });
        let code = &mut m.function_defs[i as usize].code.as_mut().unwrap().code;

        // create reference id
        code.push(Bytecode::CopyLoc(0));
        code.push(Bytecode::StLoc(1));
        // create NUM_CHLIDREN children of id
        for _ in 0..NUM_CHILDREN {
            code.push(Bytecode::CopyLoc(1));
            code.push(Bytecode::LdU64(0));
            code.push(Bytecode::VecImmBorrow(SignatureIndex(3)));
        }
        // then do a whole lot of copylocs on that reference
        for _ in 0..NUM_COPYLOCS {
            code.push(Bytecode::CopyLoc(1));
            code.push(Bytecode::Pop);
        }
        for _ in 0..NUM_CHILDREN {
            code.push(Bytecode::Pop);
        }

        code.push(Bytecode::Ret);
    }

    let result = move_bytecode_verifier::verify_module_with_config_for_test(
        "test_copyloc_pop",
        &production_config(),
        &m,
    );
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::CONSTRAINT_NOT_SATISFIED
    );
}
