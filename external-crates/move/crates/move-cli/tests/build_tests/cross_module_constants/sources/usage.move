module A::usage {
    use A::defn;

    // folded at compile time
    const DOUBLE: u64 = defn::MAX * 2;

    // compiled as calls to the getters synthesized in A::defn
    public fun max(): u64 { defn::MAX }
    public fun bytes(): vector<u8> { defn::BYTES }
    public fun double(): u64 { DOUBLE }

    #[test]
    fun check() {
        assert!(max() == 100);
        assert!(bytes() == b"hello");
        assert!(double() == 200);
    }
}
