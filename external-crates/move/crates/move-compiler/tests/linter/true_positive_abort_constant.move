module 0x42::M {
    const ERROR_NOT_OWNER: u64 = 2;

    // True Positives: These should trigger the lint
    fun test_lint_abort_incorrect() {
        abort 100 // Should trigger: using a numeric literal
    }

    fun test_lint_abort_complex_expression() {
        abort 1 + 2 // Should trigger
    }

    // Additional edge cases
    fun test_lint_abort_zero() {
        abort 0 // Should trigger
    }

    fun test_lint_abort_hex_literal() {
        abort 0x1F // Should trigger
    }

    fun test_lint_abort_addition_with_constant() {
        abort 1 + ERROR_NOT_OWNER // Should trigger
    }

    fun test_lint_abort_addition_all_constants() {
        abort ERROR_NOT_OWNER + ERROR_NOT_OWNER // Should trigger
    }

    fun test_lint_abort_named_value() {
        let x = 10;
        abort x
    }

    fun test_lint_abort_dynamic_value(error_code: u64) {
        abort error_code // trigger, since it's a dynamic value, not a constant
    }

    fun test_lint_assert_literal() {
        assert!(false, 2); // Should trigger: using a numeric literal
    }

    fun test_lint_assert_addition() {
        assert!(false, 1 + 1); // Should trigger
    }

    fun test_lint_assert_hex_literal() {
        assert!(false, 0xC0FFEE); // Should trigger
    }

    fun test_lint_assert_addition_with_constant() {
        assert!(false, 1 + ERROR_NOT_OWNER); // Should trigger
    }

    fun test_lint_assert_addition_all_constants() {
        assert!(false, ERROR_NOT_OWNER + ERROR_NOT_OWNER) // Should trigger
    }

    fun test_lint_assert_named_value() {
        let x = 10;
        assert!(false, x); // Should trigger
    }

    fun test_lint_assert_function_call() {
        assert!(true, get_error_code()); // trigger, since it's a function call
    }

    // Helper function for testing
    fun get_error_code(): u64 {
        ERROR_NOT_OWNER
    }
}
