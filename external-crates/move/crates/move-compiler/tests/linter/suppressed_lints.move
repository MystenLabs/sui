module 0x42::M {

    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42; // Should trigger a warning

    #[allow(lint(excessive_params))]
    public fun badFunction(_a: u64, _b: u64, _c: u64, _d: u64, _e: u64, _f: u64) {
        // Function body
    }
}
