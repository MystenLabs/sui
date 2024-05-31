module 0x42::M {
    const ERROR_NOT_OWNER: u64 = 2;

    #[allow(lint(abort_constant))]
    fun test_lint_abort_incorrect() {
        abort 100
    }
}
