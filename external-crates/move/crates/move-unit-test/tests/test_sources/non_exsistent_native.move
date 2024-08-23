module 0x1::M {
    native fun foo();

    #[test]
    fun non_existent_native() {
        foo()
    }
}
