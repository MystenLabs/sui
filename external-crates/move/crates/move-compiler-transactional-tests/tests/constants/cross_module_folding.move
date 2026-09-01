//# init --edition 2024.alpha

//# publish
module 0x42::a {
    public(package) const BASE: u64 = 10;
}

module 0x42::b {
    use 0x42::a;

    public(package) const DOUBLE: u64 = a::BASE * 2;
}

module 0x42::c {
    use 0x42::a;
    use 0x42::b;

    const QUAD: u64 = b::DOUBLE * 2;
    const MIXED: u64 = a::BASE + b::DOUBLE + QUAD;

    public fun check() {
        // a chain through two modules
        assert!(QUAD == 40, 0);
        // a constant mixing every link of the chain
        assert!(MIXED == 70, 1);
        assert!(b::DOUBLE == 20, 2);
    }
}

//# run 0x42::c::check
