module PreTypeError::M1 {
    // this pre-typing (but post-parsing) error should not prevent M2 from building symbolication
    // information
    fun wrong(): address {
        some_var
    }
}
