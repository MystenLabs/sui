module 0x42::foo {
    public fun inc(x: u64): u64 {
        x + 1
    }
}

module 0x43::foo_spec {
    #[spec_only]
    use prover::prover::{ensures, requires};
    use 0x42::foo::inc;

    #[spec]
    public fun foo_scenario_spec(x: u64): u64 {
        requires(x < std::u64::max_value!());
        let res = inc(x);
        let x_int = x.to_int();
        let res_int = res.to_int();
        ensures(res_int == x_int.add(1u64.to_int()));
        res
    }
}
