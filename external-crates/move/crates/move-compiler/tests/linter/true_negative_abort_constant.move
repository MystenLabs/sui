module 0x42::M {
    const ERR_INVALID_ARGUMENT: u64 = 1;
    const ERROR_NOT_OWNER: u64 = 2;
    const COMPLEX_ERROR: u64 = 3 + 4;

    fun test_lint_abort_correct() {
        abort ERR_INVALID_ARGUMENT // Correct: using a named constant
    }

    fun test_lint_assert_correct() {
        assert!(false, ERROR_NOT_OWNER); // Correct: using a named constant
    }

    fun test_lint_abort_complex_constant() {
        abort COMPLEX_ERROR // Correct: using a more complex constant expression
    }
}
