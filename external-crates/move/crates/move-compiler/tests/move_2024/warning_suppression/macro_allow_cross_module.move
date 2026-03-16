// Tests that #[allow(...)] on a macro works when called from a different module.

module a::provider {
    #[allow(unused_variable)]
    public macro fun make_val(): u64 {
        let unused = 99u64;
        42u64
    }

    #[allow(unused_variable, unused_assignment)]
    public macro fun make_val2(): u64 {
        let mut unused = 99u64;
        unused = 100u64;
        42u64
    }
}

module a::consumer {
    use a::provider::{make_val, make_val2};

    fun use_it(): u64 {
        make_val!()
    }

    fun use_it2(): u64 {
        make_val2!()
    }
}
