module 0x6::M {
    use std::unit_test;

    #[test]
    fun poison_call_OLD() {
        unit_test::create_signers_for_testing(0);
    }

    #[test]
    fun poison_call() {
        unit_test::poison();
    }
}
