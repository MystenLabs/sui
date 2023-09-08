module B::usage {
    use A::defn;
    public fun usage(): u64 { defn::definition() }
}
