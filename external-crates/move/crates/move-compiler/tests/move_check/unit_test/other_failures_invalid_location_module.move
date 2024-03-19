// tests non-abort related execution failures
// invalid module in location
module 0x1::m {

    #[test]
    #[expected_failure(arithmetic_error, location=0x2::m)]
    fun u1() { }

}
