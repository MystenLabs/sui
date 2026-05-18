// Error: 'all' set as both 'deny' and 'allow'.
#[deny(all)]
#[allow(all)]
module 0x42::m {
    fun foo(a: u64) {
        let x;
    }
}
