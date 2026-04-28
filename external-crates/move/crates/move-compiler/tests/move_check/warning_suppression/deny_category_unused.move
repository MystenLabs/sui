// Test that #[deny(unused)] at the category level upgrades all unused
// warnings to errors.
module 0x42::m {
    #[deny(unused)]
    fun foo() {
        let x = 0u64;
    }
}
