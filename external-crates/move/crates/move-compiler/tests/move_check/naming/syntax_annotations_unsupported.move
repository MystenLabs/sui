module a::invalid0 {

    struct S has drop { t: vector<u64> }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t(s: &S, i: u64): &u64 { abort 0 }

    #[allow(unused_variable)]
    #[syntax(index)]
    public fun lookup_t_mut(s: &mut S, i: u64): &mut u64 { abort 0 }

}

module a::invalid1 {

    #[syntax(index)]
    struct S has drop { t: vector<u64> }

    #[allow(unused_variable)]
    #[syntax(for)]
    public fun for_t(s: &S, i: u64): &mut u64 { abort 0 }

    #[allow(unused_variable)]
    #[syntax(assign)]
    public fun assign_t(s: &mut S, i: u64): &mut u64 { abort 0 }

    #[allow(unused_variable)]
    #[syntax(nonsense)]
    public fun nonsense_t(s: &mut S, i: u64): &mut u64 { abort 0 }

}


