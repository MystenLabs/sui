// Tests that #[allow(unused_variable)] on a macro propagates to expanded code.

module a::m {
    #[allow(unused_variable)]
    macro fun unused_var(): u64 {
        let x = 5u64;
        let y = 10u64;
        y
    }

    fun call_it(): u64 {
        unused_var!()
    }
}

module a::n {
    #[allow(unused_variable)]
    macro fun unused_var2(): u64 {
        let a = 1u64;
        let b = 2u64;
        let c = 3u64;
        c
    }

    fun call_it(): u64 {
        unused_var2!()
    }
}
