module 0x42::empty_loop_lint_test {
    // True Positive Cases
    public fun true_positive_empty_while() {
        while (true) {}
    }

    public fun true_positive_empty_loop() {
        loop {}
    }

    public fun true_positive_loop_with_empty_block() {
        loop {
            // Empty block
        }
    }
}
