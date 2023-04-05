address 0x1 {
module M {
    native fun foo();

    #[test]
    fun non_existent_native() {
        foo()
    }
}
}
