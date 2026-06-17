// Test that an inner #[warn(unused_variable)] overrides an outer
// #[deny(unused)] category-level deny, restoring the warning level.
#[deny(unused)]
module 0x42::m {
    #[warn(unused_variable)]
    fun foo() {
        let x = 0u64;
    }
}
