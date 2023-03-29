// tests non-abort related execution failures
// missing or invalid location
module 0x1::m {

    #[test]
    #[expected_failure(vector_error)]
    fun t0() { }

    #[test]
    #[expected_failure(arithmetic_error)]
    fun t1() { }

    #[test]
    #[expected_failure(out_of_gas)]
    fun t2() { }

    #[test]
    #[expected_failure(major_status=4004)]
    fun t3() { }

    #[test]
    #[expected_failure(major_status=4016, minor_code=0)]
    fun t4() { }

    #[test]
    #[expected_failure(vector_error, location=x)]
    fun u0() { }

    #[test]
    #[expected_failure(out_of_gas, location=0x1::m::t0)]
    fun u2() { }

    #[test]
    #[expected_failure(major_status=4004, location=self)]
    fun u3() { }

    #[test]
    #[expected_failure(major_status=4016, minor_status=0, location=0)]
    fun u4() { }

}
