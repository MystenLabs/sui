// flag: --vector-theory=SmtArrayExt
module extensions::type_info {
    use std::string;

    // these are mocks of the type reflection scheme
    public native fun type_name<T>(): string::String;
}

module 0x42::test {
    use extensions::type_info;
    use std::string;

    struct MyTable<phantom K, phantom V> {}

    fun test_type_name_symbolic<T>(): string::String {
        type_info::type_name<MyTable<T, T>>()
    }
    spec test_type_name_symbolic {
        ensures result.bytes != b"vector<bool>";
        ensures result != type_info::type_name<vector<T>>();
    }
}

module 0x43::test {
    use std::type_name;

    struct Pair<phantom K, phantom V> {}

    fun test_type_name_symbolic<T>(): type_name::TypeName {
        type_name::get<Pair<T, T>>()
    }
    spec test_type_name_symbolic {
        ensures result.name.bytes != b"vector<bool>";
        ensures result != type_name::get<vector<T>>();
    }
}
