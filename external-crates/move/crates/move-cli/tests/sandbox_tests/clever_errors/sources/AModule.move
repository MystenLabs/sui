module std::AModule {

    #[error(code = 0)]
    const EIsThree: vector<u8> = b"EIsThree";

    #[error(code = 1)]
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
}
