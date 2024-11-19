// check that modules that are annotated as test_only are filtered out correctly
#[test_only]
module 0x1::M {
    public fun foo() {}
}

module 0x1::Tests {
    use 0x1::M;

    fun bar() {
        M::foo()
    }
}
