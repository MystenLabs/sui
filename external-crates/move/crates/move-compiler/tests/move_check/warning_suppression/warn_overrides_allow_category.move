// Test that an inner #[warn(unused_variable)] overrides an outer
// #[allow(unused)] category-level suppression.
#[allow(unused)]
module 0x42::m {
    #[warn(unused_variable)]
    fun foo() {
        let x = 0u64;
    }
}
