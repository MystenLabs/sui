module A::defn {
    public(package) const MAX: u64 = 100;
    public(package) const BYTES: vector<u8> = b"hello";
    // used only from another module's test: 'build' warns it unused, 'test' does not
    public(package) const TEST_USED: u64 = 9;
}
