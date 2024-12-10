// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::dev_utils::{
    gas_schedule::GasStatus, in_memory_test_adapter::InMemoryTestAdapter, storage::StoredPackage,
    vm_test_adapter::VMTestAdapter as _,
};
use move_binary_format::file_format::{
    empty_module, Bytecode::*, CodeUnit, FunctionDefinition, FunctionHandle, FunctionHandleIndex,
    IdentifierIndex, Signature, SignatureIndex, SignatureToken::*, Visibility,
};

// #[test]
// TODO: Determine what this was trying to test and fix it.
#[allow(dead_code)]
fn leak_with_abort() {
    let mut locals = vec![U128, MutableReference(Box::new(U128))];
    // Make locals bigger so each leak is bigger
    // 128 is limit for aptos
    for _ in 0..100 {
        locals.push(U128);
    }
    let mut m = empty_module();
    m.version = 6;
    m.signatures = vec![Signature(vec![]), Signature(locals)];
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
                // leak
                LdU128(Box::new(0)),
                StLoc(0),
                MutBorrowLoc(0),
                StLoc(1),
                // abort
                LdU64(0),
                Abort,
            ],
        }),
    }];
    let module_id = m.self_id();
    let fname = m.identifiers[0].clone();

    move_bytecode_verifier::verify_module_unmetered(&m).expect("verify failed");

    let mut adapter = InMemoryTestAdapter::new();
    let pkg = StoredPackage::from_modules_for_testing(*module_id.address(), vec![m]).unwrap();
    let linkage = pkg.linkage_context.clone();
    adapter
        .publish_package(*module_id.address(), pkg.into_serialized_package())
        .unwrap();

    let mut session = adapter.make_vm(linkage).unwrap();

    for _ in 0..100_000 {
        let _ = session.execute_entry_function(
            &module_id,
            &fname,
            vec![],
            Vec::<Vec<u8>>::new(),
            &mut GasStatus::new_unmetered(),
        );
    }

    let mem_stats = memory_stats::memory_stats().unwrap();
    assert!(mem_stats.physical_mem < 200000000);
}
