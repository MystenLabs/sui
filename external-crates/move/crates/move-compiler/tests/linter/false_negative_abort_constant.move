module 0x42::M {
    const ERR_INVALID_ARGUMENT: u64 = 1;

    fun test_lint_abort_complex_expression() {
        abort 1 + 2 // Should ideally trigger, but might be missed due to complexity
    }

    fun test_lint_assert_constant_expression() {
        assert!(false, 1 + 1); // Should ideally trigger, but might be missed
    }

    // Additional edge cases
    fun test_lint_abort_zero() {
        abort 0 // Should trigger: even though 0 is special, it's still a literal
    }

    fun test_lint_assert_bool_literal() {
        assert!(false, true); // Interesting case: using a bool literal instead of u64
    }

    fun test_lint_abort_hex_literal() {
        abort 0x1F // Should trigger: hex literals are still literals
    }
}
