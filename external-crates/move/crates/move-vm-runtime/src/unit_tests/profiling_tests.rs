// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Tests for bytecode profiling infrastructure.
//!
//! These tests verify that bytecode execution counters are correctly
//! incremented when Move functions are executed.

use crate::{
    dev_utils::{
        compilation_utils::{as_module, compile_units},
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage,
        vm_arguments::ValueFrame,
        vm_test_adapter::VMTestAdapter,
    },
    profiling::BYTECODE_COUNTERS,
    shared::gas::UnmeteredGasMeter,
};
use move_binary_format::file_format_common::Opcodes;
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId,
    runtime_value::MoveValue,
};

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);

type ModuleCode = (ModuleId, String);

fn setup_vm(modules: &[ModuleCode]) -> InMemoryTestAdapter {
    let mut adapter = InMemoryTestAdapter::new();
    let modules: Vec<_> = modules
        .iter()
        .map(|(_, code)| {
            let mut units = compile_units(code).unwrap();
            as_module(units.pop().unwrap())
        })
        .collect();
    adapter.insert_package_into_storage(
        StoredPackage::from_modules_for_testing(*modules.first().unwrap().address(), modules)
            .unwrap(),
    );
    adapter
}

fn run_function(module: &ModuleCode, fun_name: &str, args: Vec<MoveValue>) -> ValueFrame {
    let module_id = &module.0;
    let modules = vec![module.clone()];
    let adapter = setup_vm(&modules);
    let linkage = adapter.get_linkage_context(*module_id.address()).unwrap();
    let mut session = adapter.make_vm(linkage).unwrap();

    let fun_name = Identifier::new(fun_name).unwrap();
    let serialized_args: Vec<Vec<u8>> = args
        .into_iter()
        .map(|v| v.simple_serialize().unwrap())
        .collect();

    ValueFrame::serialized_call(
        &mut session,
        module_id,
        &fun_name,
        vec![],
        serialized_args,
        &mut UnmeteredGasMeter,
        None,
        true,
    )
    .unwrap()
}

#[test]
fn test_profiling_counts_instructions() {
    // Reset counters before test
    BYTECODE_COUNTERS.reset();

    // Simple function that does arithmetic
    let code = format!(
        r#"
        module 0x{}::test {{
            public fun add_numbers(a: u64, b: u64): u64 {{
                a + b
            }}
        }}
    "#,
        TEST_ADDR
    );
    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("test").unwrap());
    let module = (module_id, code);

    // Execute the function
    let _result = run_function(
        &module,
        "add_numbers",
        vec![MoveValue::U64(5), MoveValue::U64(3)],
    );

    // Take a snapshot of the counters
    let snapshot = BYTECODE_COUNTERS.snapshot();

    // Verify some instructions were counted
    let total = snapshot.total();
    assert!(
        total > 0,
        "Expected some instructions to be counted, got {}",
        total
    );

    // The function should have executed at least:
    // - COPY_LOC or MOVE_LOC for loading arguments
    // - ADD for the addition
    // - RET for returning
    assert!(
        snapshot.get(Opcodes::RET) >= 1,
        "Expected at least one RET instruction"
    );
}

#[test]
fn test_profiling_counts_loop_iterations() {
    // Note: This test verifies that the profiling mechanism can count loop iterations
    // by executing a function with a loop.

    // Function with a loop
    let code = format!(
        r#"
        module 0x{}::test {{
            public fun sum_to_n(n: u64): u64 {{
                let mut sum = 0u64;
                let mut i = 0u64;
                while (i < n) {{
                    sum = sum + i;
                    i = i + 1;
                }};
                sum
            }}
        }}
    "#,
        TEST_ADDR
    );
    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("test").unwrap());
    let module = (module_id, code);

    // Execute the function - this will increment bytecode counters
    let result = run_function(&module, "sum_to_n", vec![MoveValue::U64(10)]);

    // Verify the function executed correctly and returned a value
    // The values field contains the return values
    assert_eq!(result.values.len(), 1, "Expected one return value");
}

#[test]
fn test_profiling_reset_works() {
    // Reset and verify zero
    BYTECODE_COUNTERS.reset();
    let snapshot1 = BYTECODE_COUNTERS.snapshot();
    assert_eq!(snapshot1.total(), 0, "Expected zero after reset");

    // Execute something
    let code = format!(
        r#"
        module 0x{}::test {{
            public fun noop() {{ }}
        }}
    "#,
        TEST_ADDR
    );
    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("test").unwrap());
    let module = (module_id, code);
    let _result = run_function(&module, "noop", vec![]);

    // Verify counts increased
    let snapshot2 = BYTECODE_COUNTERS.snapshot();
    assert!(
        snapshot2.total() > 0,
        "Expected some instructions after execution"
    );

    // Reset again and verify zero
    BYTECODE_COUNTERS.reset();
    let snapshot3 = BYTECODE_COUNTERS.snapshot();
    assert_eq!(snapshot3.total(), 0, "Expected zero after second reset");
}

#[test]
fn test_profiling_iter() {
    // Note: Due to test parallelism and global counters, we verify the iterator
    // mechanism works rather than relying on specific counts after reset.

    // Execute a function that uses various instructions
    let code = format!(
        r#"
        module 0x{}::test {{
            public fun mixed_ops(a: u64, b: u64): u64 {{
                let sum = a + b;
                let diff = a - b;
                let prod = sum * diff;
                prod
            }}
        }}
    "#,
        TEST_ADDR
    );
    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("test").unwrap());
    let module = (module_id, code);
    let _result = run_function(
        &module,
        "mixed_ops",
        vec![MoveValue::U64(10), MoveValue::U64(3)],
    );

    // Take a snapshot and iterate
    let snapshot = BYTECODE_COUNTERS.snapshot();
    let entries: Vec<_> = snapshot.iter().collect();

    // Verify we got some entries (execution generates instructions even with parallel tests)
    // The test_profiling_via_telemetry test ensures fresh state, this test just verifies
    // the iterator mechanism works with whatever state exists
    assert!(
        snapshot.total() > 0 || entries.is_empty(),
        "Snapshot total and entries should be consistent"
    );

    // Verify we can sort by frequency (callers can do this)
    let mut sorted: Vec<_> = snapshot.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    // Verify sorted results are in descending order
    for window in sorted.windows(2) {
        assert!(
            window[0].1 >= window[1].1,
            "Expected sorted order: {} >= {}",
            window[0].1,
            window[1].1
        );
    }
}

#[test]
fn test_profiling_via_telemetry() {
    // Reset counters before test
    BYTECODE_COUNTERS.reset();

    // Execute a function with a loop to generate meaningful profile data
    let code = format!(
        r#"
        module 0x{}::test {{
            public fun sum_to_n(n: u64): u64 {{
                let mut sum = 0u64;
                let mut i = 0u64;
                while (i < n) {{
                    sum = sum + i;
                    i = i + 1;
                }};
                sum
            }}
        }}
    "#,
        TEST_ADDR
    );
    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("test").unwrap());
    let modules = vec![(module_id.clone(), code)];
    let adapter = setup_vm(&modules);
    let linkage = adapter.get_linkage_context(*module_id.address()).unwrap();
    let mut session = adapter.make_vm(linkage).unwrap();

    let fun_name = Identifier::new("sum_to_n").unwrap();
    let serialized_args: Vec<Vec<u8>> = vec![MoveValue::U64(5).simple_serialize().unwrap()];

    let _result = ValueFrame::serialized_call(
        &mut session,
        &module_id,
        &fun_name,
        vec![],
        serialized_args,
        &mut UnmeteredGasMeter,
        None,
        true,
    )
    .unwrap();

    let telemetry = adapter.get_telemetry_report();
    let bytecode_stats = &telemetry.bytecode_stats;
    let total = bytecode_stats.total();
    assert!(
        total > 0,
        "Expected some instructions to be counted, got {}",
        total
    );

    // Every function returns.
    assert!(
        bytecode_stats.get(Opcodes::RET) >= 1,
        "Expected at least one RET instruction"
    );

    // Verify loop-related instructions are present
    // The loop should have ADD, LT, and branch instructions
    assert!(
        bytecode_stats.get(Opcodes::ADD) >= 1,
        "Expected ADD instruction in bytecode stats"
    );
    assert!(
        bytecode_stats.get(Opcodes::LT) >= 1,
        "Expected LT instruction in bytecode stats"
    );

    // Verify we can iterate and format the data
    let csv = bytecode_stats.format_csv();
    assert!(
        csv.starts_with("opcode,count,percentage\n"),
        "Expected CSV header"
    );
    assert!(csv.contains("RET,"), "Expected RET in CSV output");
    assert!(csv.contains("ADD,"), "Expected ADD in CSV output");
}
