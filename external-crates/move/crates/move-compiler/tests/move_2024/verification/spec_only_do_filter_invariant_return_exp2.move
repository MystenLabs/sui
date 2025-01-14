// do not filter out spec-only code when it would break the ast, like being the return expression

module a::m {
    #[spec_only]
    use prover::prover::{invariant};

    public fun foo() {
        if(true) {
            invariant!(something)
        };
    }
}
