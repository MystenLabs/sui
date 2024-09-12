module 0x42::redundant_conditional_tests {

    #[allow(lint(redundant_conditional))]
    public fun test_lint_suppression(condition: bool) {
        let _ = if (condition) { true } else { false };
    }
}
