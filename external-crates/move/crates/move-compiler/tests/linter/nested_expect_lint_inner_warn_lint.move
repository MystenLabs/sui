// Test that an inner #[warn(lint(...))] overrides an outer #[expect(lint(...))],
// causing the lint to fire as a warning and the outer expect to be unfulfilled.
#[expect(lint(constant_naming))]
module 0x42::m {
    #[warn(lint(constant_naming))]
    const Another_BadName: u64 = 42;
}
