module A::usage {
    use A::defn;

    const DOUBLE: u64 = defn::MAX * 2;

    public fun max(): u64 { defn::MAX }
    public fun bytes(): vector<u8> { defn::BYTES }
    public fun double(): u64 { DOUBLE }

    #[test]
    fun check() {
        assert!(max() == 100);
        assert!(bytes() == b"hello");
        assert!(double() == 200);
        assert!(defn::TEST_USED == 9);
    }

    const FAIL_CODE: u64 = defn::MAX + 1;

    public fun fail() { abort FAIL_CODE }

    #[test]
    #[expected_failure(abort_code = FAIL_CODE)]
    fun expect_folded_code() { fail() }
}
