// Test #[deny] at module level: all warnings of the specified kind become errors.
#[deny(unused_variable)]
module 0x42::m {
    fun foo(a: u64) {
        let x;
    }

    fun bar(b: u64) {
        let y;
    }
}
