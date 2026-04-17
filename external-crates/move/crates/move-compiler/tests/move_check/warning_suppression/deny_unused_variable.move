// Test that #[deny(unused_variable)] upgrades the warning to an error.
module 0x42::m {
    #[deny(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
