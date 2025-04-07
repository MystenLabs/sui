module 0x42::foo {
    public fun inc(x: u64): u64 {
        x + 1
    }
}

module 0x52::foo {
    public fun inc(x: u64): u64 {
        x + 1
    }
}

module 0x43::foo_spec_0 {
    #[spec_only]
    use prover::prover::{ensures, requires};
    use 0x42::foo;

    #[spec(function = foo::inc)]
    public fun foo_spec_mod_fun(x: u64): u64 {
        requires(x < std::u64::max_value!());
        let res = foo::inc(x);
        let x_int = x.to_int();
        let res_int = res.to_int();
        ensures(res_int == x_int.add(1u64.to_int()));
        res
    }

    #[spec(prove, function = foo::inc)]
    public fun foo_spec_mod_fun_prove(x: u64): u64 {
        requires(x < std::u64::max_value!());
        let res = foo::inc(x);
        let x_int = x.to_int();
        let res_int = res.to_int();
        ensures(res_int == x_int.add(1u64.to_int()));
        res
    }
}

module 0x44::foo_spec_1 {
    #[spec_only]
    use prover::prover::{ensures, requires};
    use 0x42::foo::inc;

    #[spec(function = 0x42::foo::inc)]
    public fun foo_spec_pkg_mod_fun(x: u64): u64 {
        requires(x < std::u64::max_value!());
        let res = inc(x);
        let x_int = x.to_int();
        let res_int = res.to_int();
        ensures(res_int == x_int.add(1u64.to_int()));
        res
    }

    #[spec(prove, function = 0x42::foo::inc)]
    public fun foo_spec_pkg_mod_fun_prove(x: u64): u64 {
        requires(x < std::u64::max_value!());
        let res = inc(x);
        let x_int = x.to_int();
        let res_int = res.to_int();
        ensures(res_int == x_int.add(1u64.to_int()));
        res
    }
}