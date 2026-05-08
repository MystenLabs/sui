// Test that #[warn(lint(...))] with an unknown filter name produces a warning.
module 0x42::m {
    #[warn(lint(does_not_exist))]
    fun foo() {}
}
