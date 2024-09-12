module 0x42::redundant_conditional_tests {
    // Potential False Positive Cases
    // Note: These might trigger the lint, but they're actually valid use cases
    public fun test_potential_false_positive_with_side_effects(condition: &mut bool) {
        let _ = if (*condition) { *condition = false; true } else { *condition = true; false };
    }

    public fun test_potential_false_positive_different_types(condition: bool) {
        let _ = if (condition) { 1u8 } else { 0u8 };
    }
}
