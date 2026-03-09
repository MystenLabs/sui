// constants do not exist

module a::m {
    #[test, expected_failure(abort_code = a::nonsense::foo, location = a::m)]
    fun t0() {
        abort 404
    }
    #[test, expected_failure(abort_code = a::nonsense::foo, location = a::m)]
    fun t0_clever() {
        abort
    }
}
