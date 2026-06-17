// Test that an inner #[deny(lint(...))] overrides an outer #[allow(lint(...))],
// upgrading the lint diagnostic to an error.
#[allow(lint(constant_naming))]
module 0x42::m {
    #[deny(lint(constant_naming))]
    const Another_BadName: u64 = 42;
}
