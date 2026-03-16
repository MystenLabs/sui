// Tests that unused_assignment is suppressed for macro-generated variables.
// This is handled by is_from_macro_expansion() in CFGIR liveness, which
// unconditionally skips macro-generated variables (color > 0).

module a::m {
    #[allow(unused_assignment, unused_variable)]
    macro fun reassign($x: u64): u64 {
        let mut y = $x;
        y = 0u64;
        $x
    }

    fun call_it(): u64 {
        reassign!(42u64)
    }
}
