module Usage::usage {
    use Defn::defn;

    // constants cannot be used from another package, in constant definitions...
    const DOUBLE: u64 = defn::MAX * 2;

    // ...nor in function bodies
    public fun max(): u64 { defn::MAX }

    // the generated constant function is not nameable from source
    public fun call_generated(): u64 { defn::_const_MAX() }
}
