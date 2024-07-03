module std::AModule {

    #[error]
    const EIsThree: vector<u8> = b"EIsThree";

    #[error]
    const ENotFound: vector<u8> = b"Element not found";

    public fun double_except_three(x: u64): u64 {
        assert!(x != 3, EIsThree);
        x * x
    }

    public fun double_except_four(x: u64): u64 {
        assert!(x != 4);
        x * x
    }

    public fun dont_find() {
        abort ENotFound
    }

    #[test]
    fun double_two() {
        assert!(double_except_three(4) == 16, 0)
    }

    #[test]
    #[expected_failure(abort_code = EIsThree)]
    fun double_three() {
        double_except_three(3);
    }

    public macro fun abort_() {
        assert!(false);
    }

    #[test]
    fun abort_in_macro_same_module() {
        abort_!();
    }
}
