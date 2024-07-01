module 0x42::M {

    public fun test_empty_else(x: bool): bool {
        // This should trigger the lint for having an empty `else` branch
        if (x) {
            x = true;
        } else {
            // Intentionally left empty for testing
        };
        x

    }

    // Another example that might be considered, depending on the lint rule's design:
    // An `else` branch with only comments. Should it trigger the lint?
    public fun test_else_with_comments(x: bool): bool {
        if (x) {
           x = true;
        } else {
            // This else branch contains only a comment.
        };
        x

    }
}
