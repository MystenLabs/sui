// Tests that #[allow(unused)] on a macro suppresses unused warnings in expanded code.
// Note: unused_assignment is generated in CFGIR liveness, which doesn't yet see
// the macro's warning filter scope.

module a::m {
    #[allow(unused)]
    macro fun lots_of_unused(): u64 {
        let x = 5u64;
        let mut y = 10u64;
        y = 0u64;
        let z = 20u64;
        10u64
    }

    fun call_it(): u64 {
        lots_of_unused!()
    }
}
