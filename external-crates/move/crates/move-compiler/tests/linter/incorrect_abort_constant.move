module 0x42::M {

    const ERR_INVALID_ARGUMENT: u64 = 1;
    const ERROR_NOT_OWNER : u64 = 2;
    fun test_lint_abort_incorrect_code() {
        abort 100
    }

    fun test_lint_assert_incorrect_code() {
        let x = true;
        assert!(x == false, 2);
    }

    public fun test_lint_assert_correct_code() {
        let x = true;
        assert!(x == false, ERROR_NOT_OWNER);
    }
}
