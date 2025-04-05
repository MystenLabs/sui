// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        gas_schedule::GasStatus, in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage, vm_test_adapter::VMTestAdapter as _,
    },
    execution::{
        interpreter::locals::MachineHeap,
        values::{StructRef, VMValueCast, Value},
    },
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

/// Check if an intentional cycle in locals causes an infinite-depth Drop
#[test]
#[allow(clippy::drop_non_drop)]
fn leak_with_local_cycle() {
    let mut heap = MachineHeap::new();
    let mut frame = heap.allocate_stack_frame(vec![], 1).unwrap();

    let v0 = Value::make_struct(vec![Value::u8(0)]);
    let _ = frame.store_loc(0, v0);

    {
        let v0_ref = frame.borrow_loc(0).unwrap();
        let v0_box = frame.UNSAFE_copy_local_box(0);
        println!("v0 ref: {:?}", v0_ref);
        println!("box: {:?}", v0_box);

        let mut struct_ref: StructRef = VMValueCast::cast(v0_ref).unwrap();
        println!("struct ref: {:?}", struct_ref);

        struct_ref.UNSAFE_write_field_box(0, v0_box).unwrap();

        // Printing causes a Stack Overflow
        // let v0_ref = frame.borrow_loc(0).unwrap();
        // println!("v0 ref: {:?}", v0_ref);
    }

    let v0_ref = frame.borrow_loc(0).unwrap();
    let field_ref = VMValueCast::<StructRef>::cast(v0_ref)
        .unwrap()
        .borrow_field(0)
        .unwrap();
    let v0_ref = frame.borrow_loc(0).unwrap();
    // This does not stack overflow due to pointer equality checking of references.
    assert!(v0_ref.equals(&field_ref).unwrap());

    // This ensures dropping a cycle does not cause an infinite loop
    drop(frame);
    drop(heap);
}
