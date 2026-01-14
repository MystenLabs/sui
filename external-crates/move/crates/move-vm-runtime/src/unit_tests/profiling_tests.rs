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
    profiling::{BYTECODE_COUNTERS, dump_profile_info_to_file},
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
    // Note: Due to test parallelism and global counters, we test that instructions
    // are being counted rather than exact counts. The exact counting behavior is
    // tested in profiling::counters::tests.

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

    // Reset counters and immediately execute
    BYTECODE_COUNTERS.reset();
    let _result = run_function(&module, "sum_to_n", vec![MoveValue::U64(10)]);

    // Take a snapshot immediately after execution
    let snapshot = BYTECODE_COUNTERS.snapshot();

    // Verify that some instructions were executed (loop should generate many)
    let total = snapshot.total();
    assert!(
        total > 0,
        "Expected instructions to be counted for loop execution, got {}",
        total
    );

    // The loop with n=10 should have executed at least:
    // - Multiple ST_LOC for variable assignments
    // - Multiple LT comparisons
    // - Multiple ADD operations
    // - Multiple branch instructions
    // Total should be significantly more than a simple function
    assert!(
        total >= 20,
        "Expected at least 20 instructions for loop with 10 iterations, got {}",
        total
    );
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
    // Reset counters before test
    BYTECODE_COUNTERS.reset();

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

    // Verify we got some entries
    assert!(!entries.is_empty(), "Expected at least one opcode entry");

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
fn test_profiling_dump_to_file_after_execution() {
    use std::fs;

    // Use a unique temp file for this test
    let test_file = "/tmp/test_profiling_integration.profraw";

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
    let module = (module_id, code);

    // Execute the function
    let _result = run_function(&module, "sum_to_n", vec![MoveValue::U64(5)]);

    // Dump profile data to file using explicit path
    let result = dump_profile_info_to_file(test_file);
    assert!(
        result.is_ok(),
        "dump_profile_info_to_file failed: {:?}",
        result
    );

    // Read and verify file contents
    let contents = fs::read_to_string(test_file).expect("Failed to read profile file");

    // Verify CSV header
    assert!(
        contents.starts_with("opcode,count,percentage\n"),
        "Expected CSV header, got: {}",
        contents.lines().next().unwrap_or("")
    );

    // Verify that the file contains data (not just header)
    let lines: Vec<&str> = contents.lines().collect();
    assert!(
        lines.len() > 1,
        "Expected profile data in file, only got header"
    );

    // Verify RET instruction is present (every function returns)
    assert!(
        contents.contains("RET,"),
        "Expected RET instruction in profile data"
    );

    // Verify loop-related instructions are present
    // The loop should have ADD, LT, and branch instructions
    assert!(
        contents.contains("ADD,"),
        "Expected ADD instruction in profile data"
    );
    assert!(
        contents.contains("LT,"),
        "Expected LT instruction in profile data"
    );

    // Verify format: each data line should have opcode,count,percentage
    for line in lines.iter().skip(1) {
        let parts: Vec<&str> = line.split(',').collect();
        assert_eq!(parts.len(), 3, "Expected 3 columns in CSV line: {}", line);
        // Second column should be a valid number
        let count: u64 = parts[1]
            .parse()
            .unwrap_or_else(|_| panic!("Expected count to be u64: {}", parts[1]));
        assert!(count > 0, "Expected positive count for opcode {}", parts[0]);
        // Third column should be a valid percentage
        let pct: f64 = parts[2]
            .parse()
            .unwrap_or_else(|_| panic!("Expected percentage to be f64: {}", parts[2]));
        assert!(
            (0.0..=100.0).contains(&pct),
            "Expected percentage between 0 and 100: {}",
            pct
        );
    }

    // Cleanup
    let _ = fs::remove_file(test_file);
}
