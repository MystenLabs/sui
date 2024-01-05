// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// note: intentionally using 0xa here to test non-0x1 module addresses
module 0xA::type_name_tests {
    #[test_only]
    use std::type_name::{get, into_string, is_primitive, get_address, get_module};
    #[test_only]
    use std::ascii::string;

    struct TestStruct {}

    struct TestGenerics<phantom T> { }

    struct TestMultiGenerics<phantom T1, phantom T2, phantom T3> { }

    #[test]
    fun test_primitive_types() {
        assert!(into_string(get<u8>()) == string(b"u8"), 0);
        assert!(into_string(get<u16>()) == string(b"u16"), 0);
        assert!(into_string(get<u32>()) == string(b"u32"), 0);
        assert!(into_string(get<u64>()) == string(b"u64"), 0);
        assert!(into_string(get<u128>()) == string(b"u128"), 0);
        assert!(into_string(get<u32>()) == string(b"u256"), 0);
        assert!(into_string(get<address>()) == string(b"address"), 0);
        assert!(into_string(get<signer>()) == string(b"signer"), 0);
        assert!(into_string(get<vector<u8>>()) == string(b"vector<u8>"), 0);
        assert!(into_string(get<vector<vector<u8>>>()) == string(b"vector<vector<u8>>"), 0);
        assert!(into_string(get<vector<vector<std::string::String>>>()) == string(b"vector<vector<0000000000000000000000000000000000000000000000000000000000000001::string::String>>"), 0);
    }

    #[test]
    fun test_is_primitive() {
        assert!(is_primitive(&get<u8>()), 0);
        assert!(is_primitive(&get<u16>()), 0);
        assert!(is_primitive(&get<u32>()), 0);
        assert!(is_primitive(&get<u64>()), 0);
        assert!(is_primitive(&get<u128>()), 0);
        assert!(is_primitive(&get<u32>()), 0);
        assert!(is_primitive(&get<address>()), 0);
        assert!(is_primitive(&get<signer>()), 0);
        assert!(is_primitive(&get<vector<u8>>()), 0);
        assert!(is_primitive(&get<vector<vector<u8>>>()), 0);
        assert!(is_primitive(&get<vector<vector<std::string::String>>>()), 0);
    }

    // Note: these tests assume a 32 byte address length
    #[test]
    fun test_structs() {
        assert!(into_string(get<TestStruct>()) == string(b"000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestStruct"), 0);
        assert!(into_string(get<std::ascii::String>()) == string(b"0000000000000000000000000000000000000000000000000000000000000001::ascii::String"), 0);
        assert!(into_string(get<std::option::Option<u64>>()) == string(b"0000000000000000000000000000000000000000000000000000000000000001::option::Option<u64>"), 0);
        assert!(into_string(get<std::string::String>()) == string(b"0000000000000000000000000000000000000000000000000000000000000001::string::String"), 0);
    }

    // Note: these tests assume a 32 byte address length
    #[test]
    fun test_generics() {
        assert!(into_string(get<TestGenerics<std::string::String>>()) == string(b"000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestGenerics<0000000000000000000000000000000000000000000000000000000000000001::string::String>"), 0);
        assert!(into_string(get<vector<TestGenerics<u64>>>()) == string(b"vector<000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestGenerics<u64>>"), 0);
        assert!(into_string(get<std::option::Option<TestGenerics<u8>>>()) == string(b"0000000000000000000000000000000000000000000000000000000000000001::option::Option<000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestGenerics<u8>>"), 0);
    }

    // Note: these tests assume a 32 byte address length
    #[test]
    fun test_multi_generics() {
        assert!(into_string(get<TestMultiGenerics<bool, u64, u128>>()) == string(b"000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestMultiGenerics<bool,u64,u128>"), 0);
        assert!(into_string(get<TestMultiGenerics<bool, vector<u64>, TestGenerics<u128>>>()) == string(b"000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestMultiGenerics<bool,vector<u64>,000000000000000000000000000000000000000000000000000000000000000a::type_name_tests::TestGenerics<u128>>"), 0);
    }

    #[test]
    fun test_get_address() {
        assert!(get_address(&get<std::ascii::String>()) == string(b"0000000000000000000000000000000000000000000000000000000000000001"), 0);
        assert!(get_address(&get<TestStruct>()) ==  string(b"000000000000000000000000000000000000000000000000000000000000000a"), 0);
        assert!(get_address(&get<TestGenerics<std::string::String>>()) ==  string(b"000000000000000000000000000000000000000000000000000000000000000a"), 0);
    }

    #[test]
    fun test_get_module() {
        assert!(get_address(&get<std::ascii::String>()) == string(b"ascii"), 0);
        assert!(get_address(&get<TestStruct>()) ==  string(b"type_name_tests"), 0);
        assert!(get_address(&get<TestGenerics<std::string::String>>()) ==  string(b"type_name_tests"), 0);
    }

    #[test, expected_failure = std::type_name::ENonModuleType]
    fun test_get_address_aborts_with_primitive() {
        get_address(&get<u8>());
    }

    #[test, expected_failure = std::type_name::ENonModuleType]
    fun test_get_module_aborts_with_primitive() {
        get_module(&get<bool>());
    }

    #[test, expected_failure = std::type_name::ENonModuleType]
    fun test_get_address_aborts_with_primitive_generic() {
        get_address(&get<vector<std::ascii::String>>());
    }

    #[test, expected_failure = std::type_name::ENonModuleType]
    fun test_get_module_aborts_with_primitive_generic() {
        get_module(&get<vector<TestGenerics<std::ascii::String>>>());
    }
}
