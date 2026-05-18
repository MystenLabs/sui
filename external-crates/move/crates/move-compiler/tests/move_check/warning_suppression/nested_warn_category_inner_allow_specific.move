// Test that an inner #[allow(unused_variable)] overrides an outer
// #[warn(unused)] category-level warn.
#[warn(unused)]
module 0x42::m {
    #[allow(unused_variable)]
    fun foo() {
        let x = 0u64;
    }
}
