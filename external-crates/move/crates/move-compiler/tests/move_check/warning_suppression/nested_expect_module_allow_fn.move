// Documentation test: #[expect] on module, #[allow] on function, same code.
// The inner allow wins, suppressing the warning. The outer module-level expect
// is never matched and reports as unfulfilled.
#[expect(unused_variable)]
module 0x42::m {
    #[allow(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
