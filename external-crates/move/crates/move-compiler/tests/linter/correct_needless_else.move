module 0x42::M {

    // Control example: Proper use of `else` that shouldn't trigger the lint
    public fun test_else_with_content(x: bool): bool {
        if (x) {
            x = false;
        } else {
            x = true;
        };
        x
    }

}
