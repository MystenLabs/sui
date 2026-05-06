// Test that an inner #[warn(lint(...))] overrides an outer #[allow(lint(...))],
// re-enabling the lint diagnostic as a warning.
#[allow(lint(constant_naming))]
module 0x42::m {
    #[warn(lint(constant_naming))]
    const Another_BadName: u64 = 42;
}
