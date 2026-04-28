// Test that conflicting #[warn] and #[deny] on the same code at the same scope
// is detected as a conflict.
module 0x42::m {
    #[warn(unused_variable)]
    #[deny(unused_variable)]
    fun foo() {
        let x = 0u64;
    }
}
