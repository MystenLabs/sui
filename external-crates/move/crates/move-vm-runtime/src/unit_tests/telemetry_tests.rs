// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        compilation_utils::{as_module, compile_units},
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage,
        vm_test_adapter::VMTestAdapter,
    },
    execution::vm::MoveVM,
    shared::gas::UnmeteredGasMeter,
};
use move_binary_format::errors::VMResult;
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId,
};

use std::{sync::Arc, thread};

const TEST_ADDR_0: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);
const TEST_ADDR_1: AccountAddress = AccountAddress::new([43; AccountAddress::LENGTH]);

fn make_adapter() -> InMemoryTestAdapter {
    let code = format!(
        r#"
        module 0x{}::M {{
            public struct Foo has copy, drop {{ x: u64 }}
            public struct Bar<T> has copy, drop {{ x: T }}

            fun foo() {{ }}

            fun bar(): u64 {{
                let mut x = 0;
                while (x < 1000) {{
                    x = x + 1;
                }};
                x
            }}
        }}
    "#,
        TEST_ADDR_0
    );

    let mut units = compile_units(&code).unwrap();
    let m_0 = as_module(units.pop().unwrap());

    let code = format!(
        r#"
        module 0x{}::M {{
            public struct Foo has copy, drop {{ x: u64 }}
            public struct Bar<T> has copy, drop {{ x: T }}

            fun foo() {{ }}

            fun bar(): u64 {{
                let mut x = 0;
                while (x < 1000) {{
                    x = x + 1;
                }};
                x
            }}
        }}
    "#,
        TEST_ADDR_1
    );

    let mut units = compile_units(&code).unwrap();
    let m_1 = as_module(units.pop().unwrap());

    let mut adapter = InMemoryTestAdapter::new();
    let pkg_0 = StoredPackage::from_modules_for_testing(TEST_ADDR_0, vec![m_0.clone()]).unwrap();
    adapter.insert_package_into_storage(pkg_0);
    let pkg_1 = StoredPackage::from_modules_for_testing(TEST_ADDR_1, vec![m_1.clone()]).unwrap();
    adapter.insert_package_into_storage(pkg_1);
    adapter
}

fn make_vm_0(adapter: &InMemoryTestAdapter) -> MoveVM {
    let linkage = adapter.get_linkage_context(TEST_ADDR_0).unwrap();
    adapter.make_vm(linkage).unwrap()
}

fn call_foo_0(vm: &mut MoveVM) -> VMResult<()> {
    let module_id = ModuleId::new(TEST_ADDR_0, Identifier::new("M").unwrap());
    let fun_name = Identifier::new("foo").unwrap();
    vm.execute_function_bypass_visibility(
        &module_id,
        &fun_name,
        vec![],
        Vec::<Vec<u8>>::new(),
        &mut UnmeteredGasMeter,
        None,
    )?;
    Ok(())
}

fn call_bar_0(vm: &mut MoveVM) -> VMResult<()> {
    let module_id = ModuleId::new(TEST_ADDR_0, Identifier::new("M").unwrap());
    let fun_name = Identifier::new("foo").unwrap();
    vm.execute_function_bypass_visibility(
        &module_id,
        &fun_name,
        vec![],
        Vec::<Vec<u8>>::new(),
        &mut UnmeteredGasMeter,
        None,
    )?;
    Ok(())
}

fn make_vm_1(adapter: &InMemoryTestAdapter) -> MoveVM {
    let linkage = adapter.get_linkage_context(TEST_ADDR_1).unwrap();
    adapter.make_vm(linkage).unwrap()
}

fn call_foo_1(vm: &mut MoveVM) -> VMResult<()> {
    let module_id = ModuleId::new(TEST_ADDR_1, Identifier::new("M").unwrap());
    let fun_name = Identifier::new("foo").unwrap();
    vm.execute_function_bypass_visibility(
        &module_id,
        &fun_name,
        vec![],
        Vec::<Vec<u8>>::new(),
        &mut UnmeteredGasMeter,
        None,
    )?;
    Ok(())
}

fn call_bar_1(vm: &mut MoveVM) -> VMResult<()> {
    let module_id = ModuleId::new(TEST_ADDR_1, Identifier::new("M").unwrap());
    let fun_name = Identifier::new("foo").unwrap();
    vm.execute_function_bypass_visibility(
        &module_id,
        &fun_name,
        vec![],
        Vec::<Vec<u8>>::new(),
        &mut UnmeteredGasMeter,
        None,
    )?;
    Ok(())
}

#[test]
fn basic_telemetry() {
    let adapter = make_adapter();
    let mut vm = make_vm_0(&adapter);

    let telemetry = adapter.get_telemetry_report();
    // Test that we can get telemetry, and it recorded reasonable things.
    assert_eq!(telemetry.package_cache_count, 1);
    assert_eq!(telemetry.total_arena_size, 3392);
    assert_eq!(telemetry.module_count, 1);
    assert_eq!(telemetry.function_count, 2);
    assert_eq!(telemetry.type_count, 2);
    assert_eq!(telemetry.interner_size, 4096);
    assert_eq!(telemetry.load_count, 1);
    assert_eq!(telemetry.validation_count, 1);
    assert_eq!(telemetry.jit_count, 1);
    assert_eq!(telemetry.execution_count, 0);
    assert_eq!(telemetry.interpreter_count, 0);
    assert_eq!(telemetry.total_count, 1);

    let _ = call_foo_0(&mut vm);

    // === After call_foo ===
    let telemetry = adapter.get_telemetry_report();
    assert_eq!(telemetry.package_cache_count, 1);
    assert_eq!(telemetry.total_arena_size, 3392);
    assert_eq!(telemetry.module_count, 1);
    assert_eq!(telemetry.function_count, 2);
    assert_eq!(telemetry.type_count, 2);
    assert_eq!(telemetry.interner_size, 4096);
    assert_eq!(telemetry.load_count, 1); // unchanged
    assert_eq!(telemetry.validation_count, 1); // unchanged
    assert_eq!(telemetry.jit_count, 1); // unchanged
    assert_eq!(telemetry.execution_count, 1); // 0 -> 1 after call_foo
    assert_eq!(telemetry.interpreter_count, 1); // 0 -> 1 after call_foo
    assert_eq!(telemetry.total_count, 2); // increased by 1

    let _ = call_bar_0(&mut vm);

    // === After call_bar ===
    let telemetry = adapter.get_telemetry_report();
    assert_eq!(telemetry.package_cache_count, 1);
    assert_eq!(telemetry.total_arena_size, 3392);
    assert_eq!(telemetry.module_count, 1);
    assert_eq!(telemetry.function_count, 2);
    assert_eq!(telemetry.type_count, 2);
    assert_eq!(telemetry.interner_size, 4096);
    assert_eq!(telemetry.load_count, 1); // unchanged
    assert_eq!(telemetry.validation_count, 1); // unchanged
    assert_eq!(telemetry.jit_count, 1); // unchanged
    assert_eq!(telemetry.execution_count, 2); // 1 -> 2 after call_bar
    assert_eq!(telemetry.interpreter_count, 2); // 1 -> 2 after call_bar
    assert_eq!(telemetry.total_count, 3); // increased by 1
}

#[test]
fn parallel_telemetry_1() {
    // Create the shared adapter.
    let adapter = Arc::new(make_adapter());
    let num_calls = 1_000;
    let mut handles = Vec::with_capacity(num_calls);
    // Create the VM once to avoid multiple loads
    let vm = make_vm_0(&adapter);
    drop(vm);

    // Spawn 10 threads.
    for i in 0..num_calls {
        let adapter = adapter.clone();
        // Each thread will create its own VM.
        handles.push(thread::spawn(move || {
            let mut vm = make_vm_0(&adapter);
            // Alternate between call_foo and call_bar based on the thread index.
            if i % 2 == 0 {
                call_foo_0(&mut vm).expect("call_foo failed");
            } else {
                call_bar_0(&mut vm).expect("call_bar failed");
            }
        }));
    }

    // Wait for all threads to complete.
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Get the telemetry report after all parallel calls.
    let telemetry = adapter.get_telemetry_report();

    // In our basic setup, before any calls:
    //   package_cache_count:   1
    //   total_arena_size:      3392
    //   module_count:          1
    //   function_count:        2
    //   type_count:            2
    //   interner_size:         4096
    //   load_count:            1
    //   validation_count:      1
    //   jit_count:             1
    //   execution_count:       0
    //   interpreter_count:     0
    //   total_count:           1
    //
    // Each call (via call_foo or call_bar) records a transaction that increments:
    //  +1 execution_count and interpreter_count
    //  +2 total_count (+1 for vm, +1 for execution)

    // All other fields remain unchanged.
    assert_eq!(telemetry.package_cache_count, 1);
    assert_eq!(telemetry.total_arena_size, 3392);
    assert_eq!(telemetry.module_count, 1);
    assert_eq!(telemetry.function_count, 2);
    assert_eq!(telemetry.type_count, 2);
    assert_eq!(telemetry.interner_size, 4096);
    assert_eq!(telemetry.load_count, 1);
    assert_eq!(telemetry.validation_count, 1);
    assert_eq!(telemetry.jit_count, 1);
    assert_eq!(telemetry.execution_count, num_calls as u64);
    assert_eq!(telemetry.interpreter_count, num_calls as u64);
    assert_eq!(telemetry.total_count, num_calls as u64 * 2 + 1);
}

#[test]
fn parallel_telemetry_2() {
    // Create the shared adapter.
    let adapter = Arc::new(make_adapter());
    let num_calls = 20;
    let mut handles = Vec::with_capacity(num_calls);

    // Spawn 10 threads.
    for i in 0..num_calls {
        let adapter = adapter.clone();
        // Each thread will create its own VM.
        handles.push(thread::spawn(move || {
            let rand = i % 4;
            // Simulate some loads and calls before others
            thread::sleep(std::time::Duration::from_millis(i as u64 * 400));
            match rand {
                0 => {
                    let mut vm = make_vm_0(&adapter);
                    call_foo_0(&mut vm).expect("call_foo failed");
                    (2, 1)
                }
                1 => {
                    let mut vm = make_vm_1(&adapter);
                    call_foo_1(&mut vm).expect("call_foo failed");
                    (2, 1)
                }
                2 => {
                    let mut vm = make_vm_0(&adapter);
                    call_bar_0(&mut vm).expect("call_bar failed");
                    (2, 1)
                }
                3 => {
                    let mut vm = make_vm_1(&adapter);
                    call_bar_1(&mut vm).expect("call_bar failed");
                    (2, 1)
                }
                _ => unreachable!(),
            }
        }));
    }

    // Wait for all threads to complete.
    let (total_transactions, total_calls): (u64, u64) = handles
        .into_iter()
        .map(|handle| handle.join().expect("Thread panicked"))
        .fold((0, 0), |(txn, calls), (new_txn, new_calls)| {
            (txn + new_txn, calls + new_calls)
        });

    // Get the telemetry report after all parallel calls.
    let telemetry = adapter.get_telemetry_report();

    // All other fields remain unchanged.
    // assert_eq!(telemetry.package_cache_count, 2);
    assert_eq!(telemetry.total_arena_size, 6784);
    assert_eq!(telemetry.module_count, 2);
    assert_eq!(telemetry.function_count, 4);
    assert_eq!(telemetry.type_count, 4);
    assert_eq!(telemetry.interner_size, 4096);
    assert_eq!(telemetry.load_count, 2);
    assert_eq!(telemetry.validation_count, 2);
    assert_eq!(telemetry.jit_count, 2);
    assert_eq!(telemetry.execution_count, total_calls);
    assert_eq!(telemetry.interpreter_count, total_calls);
    assert_eq!(telemetry.total_count, total_transactions);
}
