module a::m {
    #[spec_only]
    use prover::prover::{invariant};
    
    public fun foo() {
        invariant!(something);
    }
}
