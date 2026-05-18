// Test: #[warn] on module, #[allow] on function. The inner allow suppresses.
#[warn(unused_variable)]
module 0x42::m {
    #[allow(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
