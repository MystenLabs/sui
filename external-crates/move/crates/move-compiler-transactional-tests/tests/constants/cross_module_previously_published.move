//# init --edition 2024.alpha

// Modules published in earlier tasks are recompiled as source dependencies, so their constants
// can be copied and folded like any other module in the compilation

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
