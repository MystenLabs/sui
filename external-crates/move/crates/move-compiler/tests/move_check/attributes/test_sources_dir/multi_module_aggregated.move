// Multiple modules in the same file lack `#[test_only]`. Exactly one warning should be emitted,
// anchored at the first offender, with the others attached as secondary labels.

module a::first {
    fun foo() {}
}

module a::second {
    fun foo() {}
}

module a::third {
    fun foo() {}
}
