// filter out let bindings calling functions from a spec_only module

module a::m {
    #[spec_only]
    use prover::prover::{ old };

    public fun foo(_x: u64) {
        let x0 = old !(_x);
    }
}
