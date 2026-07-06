// Test: #[expect] on module, #[warn] on function overrides the expect.
// The expect is never fulfilled, so it fires as unfulfilled.
#[expect(unused_variable)]
module 0x42::m {
    #[warn(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
