module 0x42::M {

    public fun valid_if_else(x: u8) {
        // This should not trigger the linter warning
        // It has meaningful conditional logic
        if (x > 10) {
            x + 1;
        } else {
            x - 1;
        }
    }

}
