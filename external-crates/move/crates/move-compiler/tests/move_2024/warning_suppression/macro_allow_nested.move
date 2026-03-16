// Tests that #[allow(...)] works with nested macro calls.

module a::m {
    #[allow(unused_variable)]
    macro fun inner(): u64 {
        let unused_inner = 1u64;
        42u64
    }

    #[allow(unused_variable)]
    macro fun outer(): u64 {
        let unused_outer = 2u64;
        inner!()
    }

    fun call_it(): u64 {
        outer!()
    }
}
