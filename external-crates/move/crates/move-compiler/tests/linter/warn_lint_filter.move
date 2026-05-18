// Test that #[warn(lint(...))] adds the "lint level defined here"
// note to the diagnostic output.
module 0x42::m {
    #[warn(lint(constant_naming))]
    const Another_BadName: u64 = 42;
}
