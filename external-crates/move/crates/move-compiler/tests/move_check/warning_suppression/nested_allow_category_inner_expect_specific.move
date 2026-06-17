// Test that an inner #[expect(unused_variable)] overrides an outer
// #[allow(unused)] category-level allow, and the expectation is fulfilled.
#[allow(unused)]
module 0x42::m {
    #[expect(unused_variable)]
    fun foo() {
        let x = 0u64;
    }
}
