module Usage::usage {
    use Defn::defn;

    // cross-package usage does not work for constants
    #[allow(unused_const)]
    const DOUBLE: u64 = defn::MAX * 2;

    // it also does not work for functions
    public fun max(): u64 { defn::MAX }
}
