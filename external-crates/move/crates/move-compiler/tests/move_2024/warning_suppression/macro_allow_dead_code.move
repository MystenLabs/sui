// Tests that #[allow(dead_code)] on a macro propagates to expanded code.

module a::m {
    #[allow(dead_code)]
    macro fun dead_after_abort(): u64 {
        let x = abort 0;
        x + 1u64
    }

    fun call_it(): u64 {
        dead_after_abort!()
    }
}

module a::n {
    #[allow(dead_code)]
    macro fun dead_after_loop(): u64 {
        loop {};
        42u64
    }

    fun call_it(): u64 {
        dead_after_loop!()
    }
}
