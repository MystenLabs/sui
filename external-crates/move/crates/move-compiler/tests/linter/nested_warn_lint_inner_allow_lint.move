// Test that an inner #[allow(lint(...))] overrides an outer #[warn(lint(...))],
// suppressing the lint diagnostic.
#[warn(lint(constant_naming))]
module 0x42::m {
    #[allow(lint(constant_naming))]
    const Another_BadName: u64 = 42;
}
