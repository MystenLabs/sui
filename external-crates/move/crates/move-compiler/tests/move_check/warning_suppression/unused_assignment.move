#[allow(unused_variable, unused_assignment)]
module 0x42::m {
    fun foo() {
        let x = 0;
    }

    fun bar() {
        let x = 1;
        assert!(x == 1, 0);
        x = 0;
    }
}

module 0x42::n {
    #[allow(unused_variable)]
    fun foo() {
        let x = 0;
    }

    #[allow(unused_variable, unused_assignment)]
    fun bar() {
        let x = 1;
        assert!(x == 1, 0);
        x = 0;
    }
}
