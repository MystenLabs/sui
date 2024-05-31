module 0x42::M {
    const ERR_INVALID_ARGUMENT: u64 = 1;

    fun test_lint_abort_dynamic_value(error_code: u64) {
        abort error_code // Might trigger, but it's a dynamic value, not a literal
    }

    fun test_lint_assert_function_call() {
        assert!(true, get_error_code()); // Might trigger, but it's a function call
    }

    // Helper function for testing
    fun get_error_code(): u64 {
        ERR_INVALID_ARGUMENT
    }
}
