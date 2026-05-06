// Test: #[warn] with no outer suppression. Behaves like the default warning
// level but adds "the lint level is defined here" pointing to the attribute.
module 0x42::m {
    #[warn(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
