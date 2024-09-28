module 0x42::empty_else_false_positive {
    public fun test_else_with_side_effect(): u64 {
        let x = 10;
        if (x > 5) {
            // Do something
        } else {
            x = 5;
        };
        x
    }

    public fun test_else_with_complex_expression() {
        let x = 10;
        if (x > 5) {
            // Do something
        } else {
            ((((x)))); // This is not truly empty, but might be seen as such by the lint
        };
    }
}
