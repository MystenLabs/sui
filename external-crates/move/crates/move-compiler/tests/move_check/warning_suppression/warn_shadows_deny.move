// Test: #[deny] on module upgrades to error, #[warn] on function restores to warning.
#[deny(unused_variable)]
module 0x42::m {
    #[warn(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
