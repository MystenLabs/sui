module 0x42::empty_else_true_positive {
    public fun test_empty_else() {
        let x = 10;
        if (x > 5) {
            // Do something
        } else {
            // Empty else branch
        };
    }

    public fun test_empty_else_with_comment() {
        let x = 10;
        if (x > 5) {
            // Do something
        } else {
            // This comment doesn't prevent the else branch from being considered empty
        };
    }
}
