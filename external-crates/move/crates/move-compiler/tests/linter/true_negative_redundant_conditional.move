module 0x42::redundant_conditional_tests {
    // True Negative Cases
    public fun test_true_negative_different_values(condition: bool) {
        let _ = if (condition) { 1 } else { 0 };
    }

    public fun test_true_negative_complex_condition(x: u64, y: u64) {
        let _ = if (x > y) { x } else { y };
    }

    // Potential False Positive Cases
    // Note: These might trigger the lint, but they're actually valid use cases
    public fun test_potential_false_positive_with_side_effects(condition: &mut bool) {
        let _ = if (*condition) { *condition = false; true } else { *condition = true; false };
    }

    public fun test_potential_false_positive_different_types(condition: bool) {
        let _ = if (condition) { 1u8 } else { 0u8 };
    }
}
