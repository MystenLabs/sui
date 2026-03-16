// Tests that #[allow(all)] on a macro suppresses all warnings in expanded code.
// Note: untyped_literal and unused_assignment are generated in later passes
// (constraint resolution and CFGIR liveness) that don't yet see the macro's
// warning filter scope.

module a::m {
    #[allow(all)]
    macro fun messy(): u64 {
        let x = 5u64;
        let mut y = 10u64;
        y = 0u64;
        10u64
    }

    fun call_it(): u64 {
        messy!()
    }
}
