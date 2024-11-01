module 0x6::M {
    #[test_only]
    public fun this_aborts() {
        abort 0
    }

    #[test]
    fun dummy_test() { }
}

module 0x7::B {

    #[test_only]
    use 0x6::M;

    #[test]
    fun failing_test() {
        M::this_aborts()
    }
}
