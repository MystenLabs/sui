// Test that #[expect] on a warning that does NOT fire produces an unfulfilled expectation warning.
#[expect(unused_variable)]
module 0x42::m {
    fun foo(a: u64): u64 {
        a
    }
}

module 0x42::n {
    #[expect(unused_variable)]
    fun foo(a: u64): u64 {
        a
    }
}
