// Test that an inner #[deny(unused_variable)] overrides an outer
// #[allow(unused)] category-level allow, upgrading the diagnostic to an error.
#[allow(unused)]
module 0x42::m {
    #[deny(unused_variable)]
    fun foo() {
        let x = 0u64;
    }
}
