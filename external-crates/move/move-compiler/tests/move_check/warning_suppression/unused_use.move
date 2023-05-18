module 0x42::x {}

#[allow(unused_use)]
module 0x42::m {
    use 0x42::x;
}

module 0x42::n {
    #[allow(unused_use)]
    const FOO: u64 = {
        use 0x42::x;
        0
    };

    #[allow(unused_use)]
    fun foo() {
        use 0x42::x;
    }
}
