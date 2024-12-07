// filter out spec_only function, while should lead to error

module a::m {
    public fun foo() {
        bar()
    }

    #[spec_only]
    public fun bar() {
    }
}
