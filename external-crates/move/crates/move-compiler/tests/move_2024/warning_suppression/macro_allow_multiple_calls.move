// Tests that #[allow(...)] on a macro works across multiple call sites.

module a::m {
    #[allow(unused_variable)]
    macro fun with_unused(): u64 {
        let tmp = 0u64;
        42u64
    }

    fun call_once(): u64 {
        with_unused!()
    }

    fun call_twice(): u64 {
        with_unused!() + with_unused!()
    }

    fun call_in_let(): u64 {
        let a = with_unused!();
        let b = with_unused!();
        a + b
    }
}
