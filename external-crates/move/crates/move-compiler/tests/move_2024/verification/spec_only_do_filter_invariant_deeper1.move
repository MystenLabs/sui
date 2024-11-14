module a::m {
    #[verify_only]
    use prover::prover::{invariant};

    public fun foo() {
        if(true) {
            invariant!(something);
        };
    }
}
