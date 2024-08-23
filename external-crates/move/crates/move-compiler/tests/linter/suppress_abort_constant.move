module 0x42::M {
    const ERROR_NOT_OWNER: u64 = 2;

    #[allow(lint(abort_without_constant))]
    fun test_lint_abort_incorrect() {
        abort 100
    }
}

#[allow(lint(abort_without_constant))]
module 0x42::M2 {
    const ERROR_NOT_OWNER: u64 = 2;

    fun test_lint_abort_incorrect() {
        abort 100
    }
}


