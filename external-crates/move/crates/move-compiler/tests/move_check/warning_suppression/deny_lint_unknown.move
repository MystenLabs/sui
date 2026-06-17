// Test that #[deny(lint(...))] with an unknown filter name produces a warning.
module 0x42::m {
    #[deny(lint(does_not_exist))]
    fun foo() {}
}
