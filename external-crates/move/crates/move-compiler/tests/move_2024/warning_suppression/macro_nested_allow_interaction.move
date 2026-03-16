// Tests interactions between #[allow(...)] on nested macro calls.
//
// When macro A calls macro B, each macro's #[allow] applies to its own expansion.
// A's allow does not suppress warnings from B's body (unless B also has its own allow),
// and B's allow does not leak into A's scope.

module a::m {
    // Inner macro has NO allow — its unused variable should warn.
    macro fun inner_no_allow(): u64 {
        let unused_inner = 0u64;
        42u64
    }

    // Inner macro has its own allow — no warning.
    #[allow(unused_variable)]
    macro fun inner_with_allow(): u64 {
        let unused_inner = 0u64;
        42u64
    }

    // Outer macro has allow, calls inner without allow.
    // The outer's allow suppresses warnings from its own body.
    // The inner's unused variable should still warn (it's at the inner's definition site).
    #[allow(unused_variable)]
    macro fun outer_allowed(): u64 {
        let unused_outer = 0u64;
        inner_no_allow!()
    }

    // Outer macro has NO allow, calls inner with allow.
    // The inner's allow suppresses the inner's warnings.
    // The outer's unused variable should still warn.
    macro fun outer_no_allow(): u64 {
        let unused_outer = 0u64;
        inner_with_allow!()
    }

    fun call_them(): u64 {
        outer_allowed!() + outer_no_allow!()
    }
}
