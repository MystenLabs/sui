// do not filter spec-only functions if there is no #spec_only use in the file
// this is to prevent non-verification code from being filtered out

module a::m {    
    public fun foo() {
        invariant!(something);
    }
}
