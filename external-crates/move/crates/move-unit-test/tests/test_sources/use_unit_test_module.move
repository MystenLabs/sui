module 0x6::M {
    use std::unit_test;

    #[test]
    fun poison_call() {
        unit_test::poison();
    }
}
