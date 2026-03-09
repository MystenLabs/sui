// constants do not exist

module a::m {
    #[test, expected_failure(abort_code = nonsense::nonsense::foo, location = a::m)]
    fun t0() {
        abort 404
    }
}
