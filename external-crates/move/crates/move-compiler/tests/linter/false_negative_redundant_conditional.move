module 0x42::redundant_conditional_tests {
    // Potential False Negative Cases
    // Note: These should ideally trigger the lint, but might not due to complexity
    public fun test_potential_false_negative_complex_block(condition: bool) {
        let _ = if (condition) {
            let x = 1;
            let y = 2;
            x < y
        } else {
            let a = 3;
            let b = 4;
            a >= b
        };
    }

    public fun test_potential_false_negative_nested_if(condition1: bool, condition2: bool) {
        let _ = if (condition1) {
            if (condition2) { true } else { false }
        } else {
            false
        };
    }
}
