module extensions::type_info {
    use std::string;

    struct TypeInfo has copy, drop, store {
        account_address: address,
        module_name: vector<u8>,
        struct_name: vector<u8>,
    }

    // these are mocks of the type reflection scheme
    public native fun type_of<T>(): TypeInfo;
    public native fun type_name<T>(): string::String;
    spec native fun spec_is_struct<T>(): bool;

    public fun account_address(type_info: &TypeInfo): address {
        type_info.account_address
    }

    public fun module_name(type_info: &TypeInfo): vector<u8> {
        type_info.module_name
    }

    public fun struct_name(type_info: &TypeInfo): vector<u8> {
        type_info.struct_name
    }
}

module 0x42::test {
    use extensions::type_info;
    use std::string;

    struct MyTable<phantom K, phantom V> {}

    fun test_type_name_concrete(): string::String {
        spec {
            assert type_info::type_name<bool>().bytes == b"bool";
        };
        type_info::type_name<MyTable<vector<bool>, address>>()
    }
    spec test_type_name_concrete {
        ensures result.bytes == b"0x42::test::MyTable<vector<bool>, address>";
    }

    fun test_type_info_concrete(): type_info::TypeInfo {
        spec {
            assert type_info::type_of<MyTable<address, u128>>().account_address == @0x42;
            assert type_info::type_of<MyTable<address, u128>>().module_name == b"test";
            assert type_info::type_of<MyTable<address, u128>>().struct_name == b"MyTable";
        };
        type_info::type_of<MyTable<vector<bool>, address>>()
    }
    spec test_type_info_concrete {
        ensures result.account_address == @0x42;
        ensures result.module_name == b"test";
        ensures result.struct_name == b"MyTable";
    }

    fun test_type_info_symbolic<T>(): type_info::TypeInfo {
        spec {
            assert type_info::type_of<T>().account_address == type_info::type_of<T>().account_address;
            assert type_info::type_of<T>().module_name == type_info::type_of<T>().module_name;
            assert type_info::type_of<T>().struct_name == type_info::type_of<T>().struct_name;
        };
        let info = type_info::type_of<T>();
        assert!(type_info::account_address(&info) == @0x42, 1);
        assert!(type_info::module_name(&info) == b"test", 2);
        assert!(type_info::struct_name(&info) == b"MyTable", 2);
        info
    }
    spec test_type_info_symbolic {
        ensures result.account_address == @0x42;
        ensures result.module_name == b"test";
        ensures result.struct_name == b"MyTable";
    }

    fun test_type_info_ignores_type_param<T>(): type_info::TypeInfo {
        type_info::type_of<MyTable<T, address>>()
    }
    spec test_type_info_ignores_type_param {
        ensures result == type_info::type_of<MyTable<address, T>>();
    }

    fun test_type_info_can_abort<T>(): type_info::TypeInfo {
        type_info::type_of<T>()
    }
    spec test_type_info_can_abort {
        // this should not pass
        aborts_if false;
    }

    fun test_type_info_aborts_if_partial<T>(): (type_info::TypeInfo, string::String) {
        (type_info::type_of<T>(), type_info::type_name<T>())
    }
    spec test_type_info_aborts_if_partial {
        pragma aborts_if_is_partial = true;
        aborts_if type_info::type_name<T>().bytes == b"bool";
        aborts_if type_info::type_name<T>().bytes == b"u64";
        aborts_if type_info::type_name<T>().bytes == b"signer";
        aborts_if type_info::type_name<T>().bytes == b"vector<address>";
    }

    fun test_type_info_aborts_if_full<T>(): (type_info::TypeInfo, string::String) {
        (type_info::type_of<T>(), type_info::type_name<T>())
    }
    spec test_type_info_aborts_if_full {
        aborts_if !type_info::spec_is_struct<T>();
    }
}

module 0x43::test {
    use std::ascii;
    use std::type_name;

    struct Pair<phantom K, phantom V> {}

    fun test_type_name_concrete_simple(): ascii::String {
        type_name::into_string(type_name::get<bool>())
    }
    spec test_type_name_concrete_simple {
        ensures result.bytes == b"bool";
    }

    fun test_type_name_concrete_vector(): ascii::String {
        type_name::into_string(type_name::get<vector<vector<u8>>>())
    }
    spec test_type_name_concrete_vector {
        ensures result.bytes == b"vector<vector<u8>>";
    }

    fun test_type_name_concrete_struct(): ascii::String {
        type_name::into_string(type_name::get<Pair<address, bool>>())
    }
    spec test_type_name_concrete_struct {
        ensures result.bytes == b"00000000000000000000000000000043::test::Pair<address, bool>";
    }
}
