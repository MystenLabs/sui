module 0x6::random_test {
    #[random_test]
    fun should_fail_test_div_mod_10(x: u64) {
        x / (x % 10);
    }

    #[random_test]
    fun should_fail_test_div_mod_10_2_vec(x: vector<vector<u64>>) {
        std::vector::length(&x) / (std::vector::length(&x) % 10);
    }

    #[random_test]
    fun should_timeout_test_timeout(b: bool) {
        while (b) {};
    }

    #[random_test, expected_failure]
    fun should_fail_test_expected_failure(b: bool) {
        if (b) {
            assert!(false, 0);
        }
    }

    #[random_test, expected_failure]
    fun should_pass_test_expected_failure_pass(_: bool) {
        assert!(false, 0);
    }
}
