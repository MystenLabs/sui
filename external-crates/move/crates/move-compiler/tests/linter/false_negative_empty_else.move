module 0x42::empty_else_false_negative {
    public fun test_else_with_only_comment() {
        let x = 10;
        if (x > 5) {
            // Do something
        } else {
            // This comment makes the else branch non-empty from a syntax perspective,
            // but it's still logically empty and the lint might miss it
        };
    }

    public fun test_else_with_empty_block() {
        let x = 10;
        if (x > 5) {
            // Do something
        } else {
            { } // This empty block might be missed by the lint
        };
    }
}
