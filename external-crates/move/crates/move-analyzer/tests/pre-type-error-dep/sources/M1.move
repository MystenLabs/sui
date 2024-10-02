module PreTypeErrorDep::M1 {

    struct SomeStruct {
        some_field: u64
    }

    public fun foo(): u64 {
        42
    }

    // this pre-typing (but post-parsing) error should not prevent M2 from building symbolication
    // information and even some symbols should be built for M1
    fun wrong(): address {
        some_var
    }
}
