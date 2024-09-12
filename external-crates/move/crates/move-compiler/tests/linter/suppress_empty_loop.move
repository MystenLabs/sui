module 0x42::empty_loop_lint_test {
    // Suppress Case
    #[allow(lint(empty_loop))]
    public fun suppressed_empty_loop() {
        loop {}
    }
}
