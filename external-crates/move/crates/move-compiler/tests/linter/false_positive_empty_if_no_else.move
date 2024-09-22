module 0x42::empty_if_no_else_false_positive {
    public fun test_if_with_side_effect(x: &mut u64) {
        if (*x > 10) {
            *x = *x + 1; // This has a side effect but might be seen as empty by the lint
        };
    }

    public fun test_if_with_complex_expression(x: u64) {
        if (x > 5) {
            ((((x)))); // This is not truly empty, but might be seen as such by the lint
        };
    }
}
