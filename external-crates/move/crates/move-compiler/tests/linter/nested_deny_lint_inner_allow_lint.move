// Test that an inner #[allow(lint(...))] overrides an outer #[deny(lint(...))],
// suppressing the lint diagnostic rather than upgrading it to an error.
#[deny(lint(constant_naming))]
module 0x42::m {
    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42;
}
