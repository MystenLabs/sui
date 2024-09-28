module 0x42::empty_if_no_else_true_positive {
    public fun test_empty_if_no_else(x: u64) {
        if (x > 10) {
            // Empty if block
        };

        let y = 5;
        if (y < 3) {
            // Another empty if block
        };
    }

    public fun test_empty_if_with_comment(x: u64) {
        if (x == 0) {
            // This comment doesn't prevent the if block from being considered empty
        };
    }
}
