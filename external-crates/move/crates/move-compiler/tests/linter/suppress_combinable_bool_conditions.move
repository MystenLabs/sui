module 0x42::suppress_combinable_bool_conditions {
    #[lint_allow(combinable_bool_conditions)]
    fun test_suppressed_cases() {
        let x = 10;
        let y = 20;

        // Case 1: Suppressed at function level
        if (x == y || x < y) {};

        // Case 2: Explicit false positive case that we want to suppress
        if (get_value() == y || get_value() < y) {};
    }

    // Helper function for demonstrating side effects
    fun get_value(): u64 {
        // Imagine this has side effects
        10
    }
}
