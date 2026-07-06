// Test that #[deny(lint(...))] upgrades a lint warning to an error.
module 0x42::m {
    #[deny(lint(constant_naming))]
    const Another_BadName: u64 = 42;
}
