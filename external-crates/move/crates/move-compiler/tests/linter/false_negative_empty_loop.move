module 0x42::empty_loop_lint_test {
    // Potential False Negative Cases
    // Note: These might not be flagged by the lint, but they could be problematic
    public fun potential_false_negative_complex_empty_loop() {
        loop {
            if (false) {
                // This block is never executed
                break
            }
        }
    }

    public fun potential_false_negative_while_with_complex_always_true_condition() {
        while (1 < 2) {
            // This is effectively an empty infinite loop
        }
    }
}
