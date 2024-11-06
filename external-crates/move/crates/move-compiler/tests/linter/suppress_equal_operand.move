module 0x42::suppress_equal_operand {
    // Example of suppressing at module level
    #[allow(lint(equal_operands))]
    fun suppressed_function() {
        let x = 10;
        let _ = x == x;  // Lint suppressed for entire function
    }

    // Example of conditional suppression based on configuration
    #[allow(lint(equal_operands))]
    fun conditionally_suppressed() {
        let x = 10;
        let _ = x == x;  // Lint suppressed only when "testing" feature is enabled
    }

}
