// Test that #[expect(unused)] at the category level is rejected because
// wildcard/category filters are not allowed in expect.
module 0x42::m {
    #[expect(unused)]
    fun foo() {
        let x = 0u64;
    }
}
