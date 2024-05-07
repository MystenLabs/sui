module a::m {

    #[test_only]
    friend a::n1;

    public fun foo(): u64 { 0 }
}

module a::n {}

module a::p {
    public fun num(): u64 { a::m::foo() }
}
