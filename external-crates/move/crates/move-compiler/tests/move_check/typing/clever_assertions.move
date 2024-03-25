module 0x42::m {
    const Bar: u8 = 42;

    #[error]
    const X: vector<u8> = b"foo";

    #[error]
    const Foo: vector<u8> = b"Foo";

    fun f() {
        assert!(false, X);
    }

    fun funny() {
        assert!(false, Foo);
    }

    fun g() {
        abort X
    }

    fun h() {
        assert!(false);
    }
}
