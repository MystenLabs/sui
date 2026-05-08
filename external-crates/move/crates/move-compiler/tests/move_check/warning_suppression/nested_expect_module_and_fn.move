// Documentation test: #[expect] on both module and function for the same code.
// The inner (function) scope resolves first and is fulfilled. The outer (module)
// scope expect is never reached and reports as unfulfilled.
#[expect(unused_variable)]
module 0x42::m {
    #[expect(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
