module Defn::defn {
    public(package) const MAX: u64 = 100;

    // the constant reference expands at each call site, where visibility is resolved
    public macro fun get(): u64 {
        MAX
    }
}
