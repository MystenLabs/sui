// Test that #[deny(all)] upgrades all warnings to errors.
#[deny(all)]
module 0x42::m {
    fun foo(a: u64) {
        let x;
    }
}
