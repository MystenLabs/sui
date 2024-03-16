module a::m {

    struct S {}

    #[allow(unused_assignment)]
    public fun t() {
        let s;
        // if we do not keep this as an assignment (instead of a pop), the bytecode verifier will
        // error
        s = S {};
        abort 0
    }
}
