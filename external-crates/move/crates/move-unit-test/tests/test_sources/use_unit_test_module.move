module 0x1::M {
    use std::unit_test;

    #[test]
    fun poison_call() {
        unit_test::create_signers_for_testing(0);
    }
}
