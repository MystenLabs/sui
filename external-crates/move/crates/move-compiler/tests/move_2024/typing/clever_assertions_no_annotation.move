module 0x42::m {
    const X: vector<u8> = b"foo";

    fun f() {
        assert!(false, X);
    }

    fun g() {
        abort X
    }
}
