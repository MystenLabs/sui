#[allow(unused_variable)]
module 0x42::m {
    fun foo(a: u64) {
        let x;
    }
}

module 0x42::n {
    #[allow(unused_variable)]
    fun foo(a: u64) {
        let x;
    }
}
