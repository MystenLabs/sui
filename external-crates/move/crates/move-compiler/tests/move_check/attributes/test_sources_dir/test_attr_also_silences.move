// `#[test]` on the module is a stronger form of `#[test_only]` and should also silence the
// `tests_missing_test_only` warning.

#[test]
module a::test_attr_also_silences {
    fun foo() {}
}
