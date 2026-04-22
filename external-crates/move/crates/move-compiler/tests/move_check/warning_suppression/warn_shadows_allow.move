// Test: #[allow] on module, #[warn] on function re-enables the warning.
#[allow(unused_variable)]
module 0x42::m {
    #[warn(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
