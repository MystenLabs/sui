module Usage::usage {
    use Defn::defn;

    // constants cannot be used from another package, in constant definitions...
    const DOUBLE: u64 = defn::MAX * 2;

    // ...nor in function bodies
    public fun max(): u64 { defn::MAX }
}
