// Test: #[warn] and #[allow] for the same code on the same item conflicts.
#[warn(unused_variable)]
#[allow(unused_variable)]
module 0x42::m {
    fun foo(a: u64) {
        let x;
    }
}
