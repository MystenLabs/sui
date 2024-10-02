module a::m {
    // Suppress Case
    #[allow(lint(loop_without_exit))]
    public fun suppressed_empty_loop() {
        loop {}
    }
}
