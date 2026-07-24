//# init --edition 2024.alpha

//# publish
module 0x42::a {
    public(package) const MAX: u64 = 100;
    public(package) const BYTES: vector<u8> = b"hello";
    public(package) const ADDR: address = @0x7;
    public(package) const HUGE: u256 =
        115792089237316195423570985008687907853269984665640564039457584007913129639935;
    public(package) const NESTED: vector<vector<u8>> = vector[b"a", b"bc"];

    // local use compiles as a constant load, alongside the generated function for the
    // cross-module uses
    public fun local_max(): u64 { MAX }
}

module 0x42::b {
    use 0x42::a;

    const DOUBLE: u64 = a::MAX * 2;
    const FOLDED: address = a::ADDR;

    public fun check() {
        // function-body uses: compiled as calls to synthesized getters in 0x42::a
        assert!(a::MAX == 100, 0);
        assert!(a::BYTES == b"hello", 1);
        // constant-definition use: folded at compile time
        assert!(DOUBLE == 200, 2);
        // local constant load and cross-module call agree
        assert!(a::local_max() == a::MAX, 3);
        // other constant types
        assert!(a::ADDR == @0x7, 4);
        assert!(
            a::HUGE ==
                115792089237316195423570985008687907853269984665640564039457584007913129639935,
            5,
        );
        assert!(a::NESTED == vector[b"a", b"bc"], 6);
        assert!(FOLDED == @0x7, 7);
    }

    public fun fail() {
        // a cross-module constant as an abort code that is actually taken
        abort a::MAX
    }
}

//# run 0x42::b::check

//# run 0x42::b::fail
