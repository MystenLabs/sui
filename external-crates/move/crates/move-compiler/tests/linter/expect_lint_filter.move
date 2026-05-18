// Test that #[expect(lint(...))] suppresses the lint and the expectation
// is fulfilled.
module 0x42::m {
    #[expect(lint(constant_naming))]
    const Another_BadName: u64 = 42;
}
