module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(needless_else))]
    public fun test_empty_else(x: bool): bool {
        // This should trigger the lint for having an empty `else` branch
        if (x) {
            x = true;
        } else {
            // Intentionally left empty for testing
        };
        x

    }    
}
