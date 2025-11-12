#[test_only]
module A::test {
    use A::mod0;

    #[test]
    fun t0() {
        assert!(mod0::test_bytestring() == b"hello world", 0);
    }

    #[test]
    fun t1() {
        assert!(mod0::test_ascii() == std::ascii::string(b"hello world"), 1);
    }

    #[test]
    fun t2() {
        assert!(mod0::test_utf8() == std::string::utf8(b"hello world"), 2);
    }

}
