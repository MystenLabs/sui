// Suppressing the warning explicitly via `#[allow(tests_missing_test_only)]` should silence it.

#[allow(tests_missing_test_only)]
module a::allow_suppresses {
    fun foo() {}
}
