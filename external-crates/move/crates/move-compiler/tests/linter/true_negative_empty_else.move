module 0x42::empty_else_true_negative {
    public fun test_if_without_else() {
        let x = 10;
        if (x > 5) {
            // Do something
        };
    }

    #[allow(unused_assignment)]
    public fun test_if_else_with_content() {
        let x = 10;
        if (x > 5) {
            // Do something
        } else {
            x = 5; // Non-empty else branch
        };
    }

    public fun test_if_else_if() {
        let x = 10;
        if (x > 5) {
            // Do something
        } else if (x < 5) {
            // Do something else
        };
    }
}
