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

    // Control example: A loop with an exit condition and non-empty body
    public fun finite_loop_with_body() {
        let counter = 0;
        while (counter < 10) {
            counter = counter + 1;
        };
    }

    // Another control example: Using a break to exit an otherwise infinite loop
    public fun infinite_loop_with_break() {
        let x = true;
        while (x) {
            break
        }
    }
}