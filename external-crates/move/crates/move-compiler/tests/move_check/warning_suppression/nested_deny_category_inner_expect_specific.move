// Test that an inner #[expect(unused_variable)] overrides an outer
// #[deny(unused)] category-level deny, fulfilling the expectation.
#[deny(unused)]
module 0x42::m {
    #[expect(unused_variable)]
    fun foo() {
        let x = 0u64;
    }
}
