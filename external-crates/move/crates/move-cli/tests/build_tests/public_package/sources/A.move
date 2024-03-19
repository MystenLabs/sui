module A::defn {
    public(package) fun definition(): u64 { 0 }
}

module A::usage {
    use A::defn;
    public fun usage(): u64 { defn::definition() }
}
