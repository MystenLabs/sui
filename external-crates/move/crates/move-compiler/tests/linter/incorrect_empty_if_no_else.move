module 0x42::M {

    public fun trigger_empty_if_no_else(x: u8) {
        // This should trigger the linter warning
        // as it's an empty `if` with no `else`
        if (x > 10) {
        }
    }
}
