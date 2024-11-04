// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::dev_utils::{
    gas_schedule::GasStatus, in_memory_test_adapter::InMemoryTestAdapter, storage::StoredPackage,
    vm_test_adapter::VMTestAdapter,
};
use move_binary_format::file_format::{
    empty_module, Bytecode::*, CodeUnit, Constant, ConstantPoolIndex, FunctionDefinition,
    FunctionHandle, FunctionHandleIndex, IdentifierIndex, Signature, SignatureIndex,
    SignatureToken::*, Visibility,
};
use move_core_types::vm_status::StatusCode;

//#[test]
// TODO: Determine what this was trying to test and fix it.
#[allow(dead_code)]
fn merge_borrow_states_infinite_loop() {
    let mut m = empty_module();
    m.version = 6;
    m.signatures = vec![
        Signature(vec![]),
        Signature(vec![
            U64,
            Vector(Box::new(U8)),
            U64,
            Vector(Box::new(U8)),
            MutableReference(Box::new(Vector(Box::new(U8)))),
            MutableReference(Box::new(U64)),
        ]),
    ];
    m.constant_pool = vec![Constant {
        type_: Vector(Box::new(U8)),
        data: vec![0],
    }];
    m.function_handles = vec![FunctionHandle {
        module: m.self_module_handle_idx,
        name: IdentifierIndex(0),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    }];
    m.function_defs = vec![FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Visibility::Private,
        is_entry: true,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(1),
            jump_tables: vec![],
            code: vec![
                LdU64(0),
                StLoc(0), // { 0 => 0 }
                LdConst(ConstantPoolIndex(0)),
                StLoc(1), // { 0 => 0, 1 => [] }
                LdU64(0),
                StLoc(2), // { 0 => 0, 1 => [], 2 => 0 }
                MutBorrowLoc(2),
                StLoc(5), // { 0 => 0, 1 => [], 2 => 0, 5 => &2 }
                LdU64(1),
                CopyLoc(5),
                WriteRef, // { 0 => 0, 1 => [], 2 => 1, 5 => &2 }
                LdConst(ConstantPoolIndex(0)),
                StLoc(3), // { 0 => 0, 1 => [], 2 => 1, 3 => [], 5 => &2 }
                MutBorrowLoc(3),
                StLoc(4), // { 0 => 0, 1 => [], 2 => 1, 3 => [], 4 => &3, 5 => &2 }
                LdConst(ConstantPoolIndex(0)),
                CopyLoc(4),
                WriteRef,
                CopyLoc(5),
                ReadRef,
                LdU64(1),
                Eq,
                BrTrue(11),
                Ret,
            ],
        }),
    }];
    move_bytecode_verifier::verify_module_unmetered(&m).expect("verify failed");

    let module_id = m.self_id();
    let fname = m.identifiers[0].clone();

    let mut adapter = InMemoryTestAdapter::new();
    let pkg = StoredPackage::from_modules_for_testing(*module_id.address(), vec![m]).unwrap();
    let linkage = pkg.linkage_context.clone();
    adapter
        .publish_package(*module_id.address(), pkg.into_serialized_package())
        .unwrap();
    let mut session = adapter.make_vm(linkage).unwrap();

    let err = session
        .execute_entry_function(
            &module_id,
            &fname,
            vec![],
            Vec::<Vec<u8>>::new(),
            &mut GasStatus::new_unmetered(),
        )
        .unwrap_err();

    assert_eq!(
        err.major_status(),
        StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR
    );
}
