module Usage::usage {
    use Defn::defn;

    // a macro body's constant reference is not visible from another package
    public fun max(): u64 { defn::get!() }
}
