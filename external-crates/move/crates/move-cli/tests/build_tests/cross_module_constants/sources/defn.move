module A::defn {
    public(package) const MAX: u64 = 100;
    public(package) const BYTES: vector<u8> = b"hello";
    // used only from another module's test
    #[allow(unused_const)]
    public(package) const TEST_USED: u64 = 9;
}
