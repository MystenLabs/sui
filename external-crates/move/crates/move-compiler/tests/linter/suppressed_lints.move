module 0x42::M {

    const ERR_INVALID_ARGUMENT: u64 = 1;
    const ERROR_NOT_OWNER : u64 = 2;

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(abort_constant))]
    fun test_lint_abort_incorrect_code() {
        abort 100
    }

}
