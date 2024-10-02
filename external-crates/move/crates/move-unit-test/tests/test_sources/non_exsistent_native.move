module 0x6::M {
    native fun foo();

    #[test]
    fun non_existent_native() {
        foo()
    }
}
