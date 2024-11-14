// filter out a call to a spec_only function, even if deeper in the body of the function

module a::m {
    #[verify_only]
    use prover::prover::invariant;

    public fun foo() {
        if(true) {
            invariant!(something);
            10;
        }
    }
}
