// invalid, non key structs are not supported, even in vectors

module a::m {
    struct S has copy, drop, store { value: u64 }

    public entry fun no(_: vector<S>) {
        abort 0
    }

}
