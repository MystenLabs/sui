module 0x1::M {
    use std::unit_test;

    #[test]
    fun poison_call() {
        unit_test::poison();
    }
}
