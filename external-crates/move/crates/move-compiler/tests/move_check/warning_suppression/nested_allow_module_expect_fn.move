// Documentation test: #[allow] on module, #[expect] on function, same code.
// The inner expect wins and is fulfilled by the warning.
#[allow(unused_variable)]
module 0x42::m {
    #[expect(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
