module 0x42::empty_loop_lint_test {
    // Potential False Positive Cases
    // Note: These might be flagged by the lint, but they're not actually problematic
    public fun potential_false_positive_loop_with_side_effect() {
        loop {
            emit_event(); // Assuming this function exists and has side effects
        }
    }

    #[allow(lint(while_true))]
    public fun potential_false_positive_while_true_with_break() {
        while (true) {
            if (some_condition()) break;
        }
    }

    // Helper functions (to avoid compilation errors)
    fun emit_event() {}
    fun some_condition(): bool { false }
}
