// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Tests for bytecode profiling infrastructure.
//!
//! Each test creates its own `InMemoryTestAdapter`, so the counters are
//! per-runtime and isolated from other tests running in parallel.

use crate::{
    dev_utils::{
        compilation_utils::{as_module, compile_units},
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage,
        vm_arguments::ValueFrame,
        vm_test_adapter::VMTestAdapter,
    },
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

/// Run a function on a fresh adapter and return the resulting adapter so the
/// caller can pull a telemetry snapshot from it.
fn run_and_return_adapter(
    module: &ModuleCode,
    fun_name: &str,
    args: Vec<MoveValue>,
) -> InMemoryTestAdapter {
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

    let _ = ValueFrame::serialized_call(
        &mut session,
        module_id,
        &fun_name,
        vec![],
        serialized_args,
        &mut UnmeteredGasMeter,
        None,
        true,
    )
    .unwrap();

    drop(session);
    adapter
}

#[test]
fn test_profiling_counts_instructions() {
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

    let adapter = run_and_return_adapter(
        &module,
        "add_numbers",
        vec![MoveValue::U64(5), MoveValue::U64(3)],
    );

    let snapshot = adapter.get_telemetry_report().bytecode_stats;
    assert!(
        snapshot.total() > 0,
        "Expected some instructions to be counted, got {}",
        snapshot.total()
    );
    assert!(
        snapshot.get(Opcodes::RET) >= 1,
        "Expected at least one RET instruction"
    );
}

#[test]
fn test_profiling_counts_loop_iterations() {
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

    let adapter = run_and_return_adapter(&module, "sum_to_n", vec![MoveValue::U64(10)]);
    let snapshot = adapter.get_telemetry_report().bytecode_stats;

    assert!(snapshot.get(Opcodes::ADD) >= 10, "Expected ADD from loop body");
    assert!(snapshot.get(Opcodes::LT) >= 10, "Expected LT from loop guard");
}

#[test]
fn test_profiling_iter_sorted_by_frequency() {
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

    let adapter = run_and_return_adapter(
        &module,
        "mixed_ops",
        vec![MoveValue::U64(10), MoveValue::U64(3)],
    );
    let snapshot = adapter.get_telemetry_report().bytecode_stats;

    let mut sorted: Vec<_> = snapshot.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    for window in sorted.windows(2) {
        assert!(window[0].1 >= window[1].1);
    }
}

#[test]
fn test_profiling_per_runtime_isolation() {
    // Two independent adapters should see independent counts.
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

    let adapter_a = run_and_return_adapter(
        &module,
        "add_numbers",
        vec![MoveValue::U64(1), MoveValue::U64(2)],
    );
    let snap_a = adapter_a.get_telemetry_report().bytecode_stats;

    // A fresh adapter has not executed anything, so its counts are zero.
    let adapter_b = setup_vm(&[module.clone()]);
    let snap_b = adapter_b.get_telemetry_report().bytecode_stats;

    assert!(snap_a.total() > 0);
    assert_eq!(snap_b.total(), 0);
}

#[test]
fn test_profiling_via_telemetry() {
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
    let module = (module_id.clone(), code);
    let adapter = run_and_return_adapter(&module, "sum_to_n", vec![MoveValue::U64(5)]);

    let telemetry = adapter.get_telemetry_report();
    let bytecode_stats = &telemetry.bytecode_stats;
    assert!(bytecode_stats.total() > 0);
    assert!(bytecode_stats.get(Opcodes::RET) >= 1);
    assert!(bytecode_stats.get(Opcodes::ADD) >= 1);
    assert!(bytecode_stats.get(Opcodes::LT) >= 1);

    let csv = bytecode_stats.format_csv();
    assert!(csv.starts_with("opcode,count,percentage\n"));
    assert!(csv.contains("RET,"));
    assert!(csv.contains("ADD,"));
}
