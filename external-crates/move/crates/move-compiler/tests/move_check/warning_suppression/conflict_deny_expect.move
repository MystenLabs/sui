// Error: same diagnostic set as both 'deny' and 'expect'.
#[deny(unused_variable)]
#[expect(unused_variable)]
module 0x42::m {
    fun foo(a: u64) {
        let x;
    }
}
