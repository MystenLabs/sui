module 0x42::empty_if_no_else_false_negative {
    public fun test_if_with_only_comment(x: u64) {
        if (x > 10) {
            // This comment makes the if block non-empty from a syntax perspective,
            // but it's still logically empty and the lint might miss it
        };
    }

    public fun test_if_with_empty_block(x: u64) {
        if (x > 5) {
            { } // This empty block might be missed by the lint
        };
    }

    public fun test_if_with_nested_empty_block(x: u64) {
        if (x > 0) {
            {
                {
                    // Nested empty blocks might be missed by the lint
                }
            }
        };
    }
}
