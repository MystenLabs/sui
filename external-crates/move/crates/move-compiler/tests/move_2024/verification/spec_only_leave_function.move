// #[verify_only] functions should be filtered out in non-verify mode
module a::m {
    // This should cause an unbound function error in non-verify mode as `bar`
    // was filtered out
    public fun foo() {
        bar()
    }

    #[verify_only]
    public fun bar() {
    }
}
