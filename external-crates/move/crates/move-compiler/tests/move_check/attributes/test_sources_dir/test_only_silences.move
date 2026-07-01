// Module under `tests/` annotated `#[test_only]` should not warn.

#[test_only]
module a::test_only_silences {
    fun foo() {}
}
