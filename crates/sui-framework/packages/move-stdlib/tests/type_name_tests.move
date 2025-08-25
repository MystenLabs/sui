// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// note: intentionally using 0xa here to test non-0x1 module addresses
#[test_only]
module 0xA::type_name_tests;

use std::type_name::{with_defining_ids, defining_id};

public struct TestStruct {}

public struct TestGenerics<phantom T> {}

public struct TestMultiGenerics<phantom T1, phantom T2, phantom T3> {}

#[test]
fun test_primitive_types() {
    assert!(with_defining_ids<u8>().as_string().as_bytes() == b"u8");
    assert!(with_defining_ids<u16>().as_string().as_bytes() == b"u16");
    assert!(with_defining_ids<u32>().as_string().as_bytes() == b"u32");
    assert!(with_defining_ids<u64>().as_string().as_bytes() == b"u64");
    assert!(with_defining_ids<u128>().as_string().as_bytes() == b"u128");
    assert!(with_defining_ids<u256>().as_string().as_bytes() == b"u256");
    assert!(with_defining_ids<address>().as_string().as_bytes() == b"address");
    assert!(with_defining_ids<vector<u8>>().as_string().as_bytes() == b"vector<u8>");
    assert!(
        with_defining_ids<vector<vector<u8>>>().as_string().as_bytes() == b"vector<vector<u8>>",
    );
    assert!(
        with_defining_ids<vector<vector<std::string::String>>>().as_string().as_bytes() == b"vector<vector<0000000000000000000000000000000000000000000000000000000000000001::string::String>>",
    );
}

#[test]
fun test_is_primitive() {
    assert!(with_defining_ids<u8>().is_primitive());
    assert!(with_defining_ids<u16>().is_primitive());
    assert!(with_defining_ids<u32>().is_primitive());
    assert!(with_defining_ids<u64>().is_primitive());
    assert!(with_defining_ids<u128>().is_primitive());
    assert!(with_defining_ids<u32>().is_primitive());
    assert!(with_defining_ids<address>().is_primitive());
    assert!(with_defining_ids<vector<u8>>().is_primitive());
    assert!(with_defining_ids<vector<vector<u8>>>().is_primitive());
    assert!(with_defining_ids<vector<vector<std::string::String>>>().is_primitive());
}

// Note: these tests assume a 32 byte address length
#[test]
fun test_structs() {
    assert!(
        with_defining_ids<TestStruct>().as_string().as_bytes() == b"000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestStruct",
    );
    assert!(
        with_defining_ids<std::ascii::String>().as_string().as_bytes() == b"0000000000000000000000000000000000000000000000000000000000000001::ascii::String",
    );
    assert!(
        with_defining_ids<std::option::Option<u64>>().as_string().as_bytes() == b"0000000000000000000000000000000000000000000000000000000000000001::option::Option<u64>",
    );
    assert!(
        with_defining_ids<std::string::String>().as_string().as_bytes() == b"0000000000000000000000000000000000000000000000000000000000000001::string::String",
    );
}

// Note: these tests assume a 32 byte address length
#[test]
fun test_generics() {
    assert!(
        with_defining_ids<TestGenerics<std::string::String>>().as_string().as_bytes() == b"000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestGenerics<0000000000000000000000000000000000000000000000000000000000000001::string::String>",
    );
    assert!(
        with_defining_ids<vector<TestGenerics<u64>>>().as_string().as_bytes() == b"vector<000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestGenerics<u64>>",
    );
    assert!(
        with_defining_ids<std::option::Option<TestGenerics<u8>>>().as_string().as_bytes() == b"0000000000000000000000000000000000000000000000000000000000000001::option::Option<000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestGenerics<u8>>",
    );
}

// Note: these tests assume a 32 byte address length
#[test]
fun test_multi_generics() {
    assert!(
        with_defining_ids<TestMultiGenerics<bool, u64, u128>>().as_string().as_bytes() == b"000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestMultiGenerics<bool,u64,u128>",
    );
    assert!(
        with_defining_ids<TestMultiGenerics<bool, vector<u64>, TestGenerics<u128>>>().as_string().as_bytes() == b"000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestMultiGenerics<bool,vector<u64>,000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestGenerics<u128>>",
    );
}

#[test]
fun test_get_address() {
    assert!(
        with_defining_ids<std::ascii::String>().address_string().as_bytes() == b"0000000000000000000000000000000000000000000000000000000000000001",
    );
    assert!(
        with_defining_ids<TestStruct>().address_string().as_bytes() == b"000000000000000000000000000000000000000000000000000000000000000a",
    );
    assert!(
        with_defining_ids<TestGenerics<std::string::String>>().address_string().as_bytes() ==  b"000000000000000000000000000000000000000000000000000000000000000a",
    );
}

#[test]
fun test_get_module() {
    assert!(with_defining_ids<std::ascii::String>().module_string().as_bytes() == b"ascii");
    assert!(with_defining_ids<TestStruct>().module_string().as_bytes() ==  b"type_name_tests");
    assert!(
        with_defining_ids<TestGenerics<std::string::String>>().module_string().as_bytes()==  b"type_name_tests",
    );
}

#[test, expected_failure(abort_code = std::type_name::ENonModuleType)]
fun test_get_address_aborts_with_primitive() {
    with_defining_ids<u8>().address_string();
}

#[test, expected_failure(abort_code = std::type_name::ENonModuleType)]
fun test_get_module_aborts_with_primitive() {
    with_defining_ids<bool>().module_string();
}

#[test, expected_failure(abort_code = std::type_name::ENonModuleType)]
fun test_get_address_aborts_with_primitive_generic() {
    with_defining_ids<vector<std::ascii::String>>().address_string();
}

#[test, expected_failure(abort_code = std::type_name::ENonModuleType)]
fun test_get_module_aborts_with_primitive_generic() {
    with_defining_ids<vector<TestGenerics<std::ascii::String>>>().module_string();
}

#[test]
fun test_defining_id() {
    assert!(defining_id<std::ascii::String>() == @1);
    assert!(defining_id<TestStruct>() == @0xa);
    assert!(defining_id<TestGenerics<std::string::String>>() == @0xa);
}

#[test, expected_failure(abort_code = std::type_name::ENonModuleType)]
fun test_defining_id_aborts_with_primitive() {
    defining_id<u8>();
}

#[test, expected_failure(abort_code = std::type_name::ENonModuleType)]
fun test_defining_id_aborts_with_primitive_generic() {
    defining_id<vector<std::ascii::String>>();
}

#[test, expected_failure(abort_code = std::type_name::ENonModuleType)]
fun test_defining_id_aborts_with_primitive_generic_nested() {
    defining_id<vector<TestGenerics<std::ascii::String>>>();
}
