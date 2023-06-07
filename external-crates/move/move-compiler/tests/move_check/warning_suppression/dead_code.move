#[allow(dead_code)]
module 0x42::m {
    fun foo() {
        loop {};
        assert!(1 == 0, 0)
    }
}

module 0x42::n {
    #[allow(dead_code)]
    fun foo() {
        let x = abort 0;
        x + 1;
    }
}
