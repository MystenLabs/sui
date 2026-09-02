//# init --edition 2024.alpha

// Constants of modules published in earlier tasks are usable from later ones

//# publish
module 0x42::a {
    public(package) const MAX: u64 = 100;
}

//# publish
module 0x42::b {
    use 0x42::a;

    const D: u64 = a::MAX + 1;

    public fun check() {
        assert!(a::MAX == 100, 0);
        assert!(D == 101, 1);
    }
}

//# run 0x42::b::check
