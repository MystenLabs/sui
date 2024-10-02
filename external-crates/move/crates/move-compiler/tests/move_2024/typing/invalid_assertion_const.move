module 0x42::m {
    const X: vector<u8> = b"X";

    fun f() {
        abort X;
    }

    fun g() {
        assert!(false, X);
    }
}
