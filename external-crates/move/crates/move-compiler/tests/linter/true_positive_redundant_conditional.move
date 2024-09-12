module 0x42::redundant_conditional_tests {
   // True Positive Cases
    public fun test_true_positive_if_true_else_false(condition: bool) {
        let _ = if (condition) { true } else { false };
    }

    public fun test_true_positive_if_false_else_true(condition: bool) {
        let _ = if (condition) { false } else { true };
    }

    public fun test_true_positive_with_block(condition: bool) {
        let _ = if (condition) {
            { true }
        } else {
            { false }
        };
    }
}
