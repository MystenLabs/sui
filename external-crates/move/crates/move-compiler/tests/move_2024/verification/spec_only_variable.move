module a::m {
    #[spec_only]
    use prover::prover::old;

    public fun foo(): u64 {
        let x_spec = 10;
        let y = x_spec;
        y
    }
}
