module Defn::defn {
    public(package) const MAX: u64 = 100;

    // expands to a constant ref at each call site
    public macro fun get(): u64 {
        MAX
    }
}
