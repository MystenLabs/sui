//# init --edition 2024.alpha

//# publish
module 0x42::a {
    public(package) const MAX: u64 = 100;
    public(package) const BYTES: vector<u8> = b"hello";
}

module 0x42::b {
    use 0x42::a;

    const DOUBLE: u64 = a::MAX * 2;

    public fun check() {
        // function-body uses: compiled as calls to synthesized getters in 0x42::a
        assert!(a::MAX == 100, 0);
        assert!(a::BYTES == b"hello", 1);
        // constant-definition use: folded at compile time
        assert!(DOUBLE == 200, 2);
        // cross-module constant as a plain abort code
        if (a::MAX == 0) abort a::MAX;
    }
}

//# run 0x42::b::check
