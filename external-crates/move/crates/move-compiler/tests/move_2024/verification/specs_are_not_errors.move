module a::m {

    // not an error
    spec foo {}

    // not an error
    #[verify_only]
    fun foo() {}
}

// not a duplicate module, not an error
spec a::m {

}
