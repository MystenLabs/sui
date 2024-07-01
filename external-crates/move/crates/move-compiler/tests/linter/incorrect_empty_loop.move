module 0x42::M {

    // This should also trigger the lint for having a condition that always evaluates to true
    public fun while_infinite_loop_always_true() {
        while (true) {
            // Intentionally left empty for testing
        }
    }

    public fun infinite_loop_always_true() {
        loop {}
    }
}
