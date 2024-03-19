// entry functions are public if called from test or test_only

module a::m {
    entry fun internal(_ :u64) {}
}

#[test_only]
module a::test_only {
    fun example() {
        // force a type error to make sure visibility is allowed
        a::m::internal(0u8)
    }
}

module a::tests {
    #[test]
    fun test() {
        // force a type error to make sure visibility is allowed
        a::m::internal(0u8)
    }
}
