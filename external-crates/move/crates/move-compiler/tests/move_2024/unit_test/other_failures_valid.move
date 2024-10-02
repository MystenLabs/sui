// tests non-abort related execution failures
module 0x1::n {}
module 0x1::m {
    #[test_only]
    use 0x1::n;

    #[test]
    #[expected_failure(vector_error, location=std::vector)]
    fun t0() { }

    #[test]
    #[expected_failure(arithmetic_error, location=n)]
    fun t1() { }

    #[test]
    #[expected_failure(out_of_gas, location=Self)]
    fun t2() { }

    #[test]
    #[expected_failure(major_status=4004, location=Self)]
    fun t3() { }

    #[test]
    #[expected_failure(major_status=4016, minor_status=0, location=Self)]
    fun t4() { }

}
