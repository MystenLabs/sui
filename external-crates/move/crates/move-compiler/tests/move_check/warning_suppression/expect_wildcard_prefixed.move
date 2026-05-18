// Test that prefixed wildcards in #[expect] are also rejected.
#[expect(lint(all))]
module 0x42::m {
    fun foo() {}
}
