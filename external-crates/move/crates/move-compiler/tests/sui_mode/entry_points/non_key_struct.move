// invalid, non key structs are not supported

module a::m {
    struct S has copy, drop, store { value: u64 }

    public entry fun no(_: S) {
        abort 0
    }
}
