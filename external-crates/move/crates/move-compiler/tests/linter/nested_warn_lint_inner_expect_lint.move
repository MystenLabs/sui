// Test that an inner #[expect(lint(...))] overrides an outer #[warn(lint(...))],
// suppressing the lint and fulfilling the expectation.
#[warn(lint(constant_naming))]
module 0x42::m {
    #[expect(lint(constant_naming))]
    const Another_BadName: u64 = 42;
}
