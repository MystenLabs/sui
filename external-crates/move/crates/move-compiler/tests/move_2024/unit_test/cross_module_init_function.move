// check that fun `init` can not be used cross module
module 0x1::M {
    fun init() { }
}

module 0x1::Tests {
    #[test]
    fun tester() {
        use 0x1::M;
        M::init();
    }
}
