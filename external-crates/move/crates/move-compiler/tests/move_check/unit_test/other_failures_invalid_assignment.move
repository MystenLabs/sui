// tests non-abort related execution failures
// invalid assignment
module 0x1::n {}
module 0x1::m {
    #[test_only]
    use 0x1::n;

    #[test]
    #[expected_failure(vector_error=0, location=std::vector)]
    fun t0() { }

    #[test]
    #[expected_failure(vector_error, minor_status=x"", location=std::vector)]
    fun t1() { }

    #[test]
    #[expected_failure(arithmetic_error=@0, location=n)]
    fun t2() { }

    #[test]
    #[expected_failure(out_of_gas=bool, location=Self)]
    fun t3() { }

    #[test]
    #[expected_failure(major_status=4004u128, location=Self)]
    fun t4() { }

    #[test]
    #[expected_failure(major_status=4016, minor_status=b"", location=Self)]
    fun t5() { }

}
