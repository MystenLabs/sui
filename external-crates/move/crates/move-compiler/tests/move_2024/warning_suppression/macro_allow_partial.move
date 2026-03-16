// Tests that #[allow(unused_variable)] on a macro suppresses unused_variable.
// unused_assignment and unused_let_mut are also suppressed because macro-generated
// variables are unconditionally excluded from those checks (via is_from_macro_expansion).

module a::m {
    #[allow(unused_variable)]
    macro fun partial(): u64 {
        let x = 5u64;
        let mut y = 10u64;
        y = 0u64;
        10u64
    }

    fun call_it(): u64 {
        partial!()
    }
}
