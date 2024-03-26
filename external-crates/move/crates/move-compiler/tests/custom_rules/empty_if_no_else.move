module 0x42::M {

    public fun trigger_empty_if_no_else(x: u8) {
        // This should trigger the linter warning
        // as it's an empty `if` with no `else`
        if (x > 10) {
        }
    }

    public fun valid_if_else(x: u8) {
        // This should not trigger the linter warning
        // It has meaningful conditional logic
        if (x > 10) {
            x + 1;
        } else {
            x - 1;
        }
    }

    public fun valid_if(x: u8) {
        // This should not trigger the linter warning
        // It has an action within the `if`
        if (x > 5) {
            x * 2;
        }
    }
}