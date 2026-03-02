module std::AModule {

    /// x was three
    const E_IS_THREE: u64 = 0;

    public fun double_except_three(x: u64): u64 {
        assert!(x != 3, E_IS_THREE);
        x * x
    }

    #[test]
    fun double_two() {
        assert!(double_except_three(4) == 16, 0)
    }

    #[test]
    #[expected_failure]
    fun double_three() {
        double_except_three(3);
    }
}
