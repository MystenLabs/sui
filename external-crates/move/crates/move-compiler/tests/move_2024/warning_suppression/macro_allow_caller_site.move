// Tests interaction between caller-site and macro-site #[allow(...)].

module a::m {
    // No allow on this macro — warnings should appear
    macro fun no_allow(): u64 {
        let x = 5u64;
        42u64
    }

    // Macro has allow for unused_variable
    #[allow(unused_variable)]
    macro fun has_allow(): u64 {
        let x = 5u64;
        42u64
    }

    // Caller-site allow suppresses macro warnings
    #[allow(unused_variable)]
    fun caller_suppresses(): u64 {
        no_allow!()
    }

    // Both macro and caller have allows
    #[allow(unused_variable)]
    fun both_suppress(): u64 {
        has_allow!()
    }
}
