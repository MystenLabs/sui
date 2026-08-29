//# init --edition 2024.alpha

//# publish
module 0x42::a {
    public(package) const MAX: u64 = 100;
    public(package) const BYTES: vector<u8> = b"hello";
    public(package) const ADDR: address = @0x7;
    public(package) const HUGE: u256 =
        115792089237316195423570985008687907853269984665640564039457584007913129639935;
    public(package) const NESTED: vector<vector<u8>> = vector[b"a", b"bc"];

    public fun local_max(): u64 { MAX }
}

module 0x42::b {
    use 0x42::a;

    const DOUBLE: u64 = a::MAX * 2;
    const FOLDED: address = a::ADDR;

    public struct S has drop { x: u64, y: vector<u8> }

    public enum En has drop { V(u64) }

    fun id(x: u64): u64 { x }

    public fun check() {
        // function-body uses
        assert!(a::MAX == 100, 0);
        assert!(a::BYTES == b"hello", 1);
        // constant-definition uses
        assert!(DOUBLE == 200, 2);
        // local and cross-module reads agree
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
        // struct and variant fields
        let s = S { x: a::MAX, y: a::BYTES };
        assert!(s.x == 100, 8);
        assert!(s.y == b"hello", 9);
        let v = match (En::V(a::MAX)) { En::V(x) => x };
        assert!(v == 100, 10);
        // mutation and direct call argument
        let mut m = 0;
        let r = &mut m;
        *r = a::MAX;
        assert!(m == 100, 11);
        assert!(id(a::MAX) == 100, 12);
    }

    public fun fail() {
        // a cross-module constant as an abort code
        abort a::MAX
    }
}

//# run 0x42::b::check

//# run 0x42::b::fail
