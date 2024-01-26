module ParseErrorDep::M1 {

    struct SomeStruct {
        some_field: u64
    }

    public fun foo(): u64 {
        42
    }

    // this parsing error should not prevent other modules from building symbolication information,
    // but since other modules depend on this one, not all information will be available but there
    // should also be no errors or assertion failures
    parse_error
}
