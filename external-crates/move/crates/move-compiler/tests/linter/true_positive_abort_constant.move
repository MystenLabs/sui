module 0x42::M {
    const ERROR_NOT_OWNER: u64 = 2;

    // True Positives: These should trigger the lint
    fun test_lint_abort_incorrect() {
        abort 100 // Should trigger: using a numeric literal
    }

    fun test_lint_assert_incorrect() {
        let x = true;
        assert!(x == false, 2); // Should trigger: using a numeric literal
    }
}
