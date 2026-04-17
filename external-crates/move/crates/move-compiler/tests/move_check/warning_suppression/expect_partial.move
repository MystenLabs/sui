// Test mixed: one expect is fulfilled (unused_variable), another is not (dead_code).
module 0x42::m {
    #[expect(unused_variable, dead_code)]
    fun foo(a: u64): u64 {
        let x;
        a
    }
}
