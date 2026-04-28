// Test that #[expect(unused)] at the category level suppresses unused
// warnings and is considered fulfilled.
module 0x42::m {
    #[expect(unused)]
    fun foo() {
        let x = 0;
    }
}
