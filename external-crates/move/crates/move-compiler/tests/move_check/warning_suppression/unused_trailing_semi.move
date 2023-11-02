#[allow(unused_trailing_semi)]
module 0x42::m {
    fun foo() {
        abort 0;
    }
}

module 0x42::n {
    #[allow(unused_trailing_semi)]
    fun foo() {
        abort 0;
    }
}
