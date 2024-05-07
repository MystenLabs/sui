#[test_only]
module migration::migration_tests {
    use migration::migration;

    #[test]
    #[expected_failure(abort_code = ::migration::validate::ErrorCode)]
    fun test_t() {
        migration::t()
    }
}
