//# init --edition 2024.alpha

//# publish
module 0x42::a {
    public(package) const ONE: u64 = 1;
    public(package) const TWO: u64 = 2;
}

module 0x42::b {
    use 0x42::a;

    // same name as a::TWO, different value
    const TWO: u64 = 20;

    public fun classify(x: u64): u64 {
        match (x) {
            a::ONE => 100,
            a::TWO => 200,
            TWO => 2000,
            _ => 0,
        }
    }

    public fun check() {
        assert!(classify(1) == 100, 0);
        assert!(classify(2) == 200, 1);
        assert!(classify(20) == 2000, 2);
        assert!(classify(3) == 0, 3);
    }
}

//# run 0x42::b::check
