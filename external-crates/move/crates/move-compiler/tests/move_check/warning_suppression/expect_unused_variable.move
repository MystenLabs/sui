// Test that #[expect(unused_variable)] suppresses the warning when the warning fires.
#[expect(unused_variable)]
module 0x42::m {
    fun foo(a: u64) {
        let x;
    }
}

module 0x42::n {
    #[expect(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
