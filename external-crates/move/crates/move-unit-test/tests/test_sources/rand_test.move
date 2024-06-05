module 0x1::rand_test {
    #[rand_test]
    fun should_fail_test_div_mod_10(x: u64) {
        x / (x % 10);
    }

    #[rand_test]
    fun should_fail_test_div_mod_10_2_vec(x: vector<vector<u64>>) {
        std::vector::length(&x) / (std::vector::length(&x) % 10);
    }

    #[rand_test]
    fun should_timeout_test_timeout(b: bool) {
        while (b) {};
    }

    #[rand_test, expected_failure]
    fun should_fail_test_expected_failure(b: bool) {
        if (b) {
            assert!(false, 0);
        }
    }

    #[rand_test, expected_failure]
    fun should_pass_test_expected_failure_pass(_: bool) {
        assert!(false, 0);
    }
}
