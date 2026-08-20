module Usage::usage {
    use Defn::defn;

    // a macro body's constant reference is not visible from another package either
    public fun max(): u64 { defn::get!() }
}
