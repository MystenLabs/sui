// Module is delivered as a test source (i.e. lives under `tests/`) but is not annotated
// `#[test_only]` (and is not `#[test]`). A `tests_missing_test_only` warning should fire.

module a::missing_test_only_warns {
    fun foo() {}
}
