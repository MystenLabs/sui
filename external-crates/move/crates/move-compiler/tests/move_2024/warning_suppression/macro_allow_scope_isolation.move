// Tests that a macro's #[allow(...)] scope does not leak into the caller's context.
// The macro's allow should suppress warnings in the expanded code, but the caller's
// own code should still be checked normally.

module a::m {
    #[allow(unused_variable)]
    macro fun allowed_macro(): u64 {
        let unused_in_macro = 0u64;
        42u64
    }

    // The caller has its own unused variable. The macro's #[allow(unused_variable)]
    // should NOT suppress the warning for `caller_unused`.
    fun caller_has_own_warning(): u64 {
        let caller_unused = 99u64;
        allowed_macro!()
    }
}
