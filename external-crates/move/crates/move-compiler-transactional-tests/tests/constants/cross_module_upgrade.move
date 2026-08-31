//# init --edition 2024.alpha

//# publish
module 0x42::a {
    public(package) const MAX: u64 = 100;
}

module 0x42::b {
    use 0x42::a;

    const DOUBLE: u64 = a::MAX * 2;

    public fun check_original() {
        assert!(a::MAX == 100, 0);
        assert!(DOUBLE == 200, 1);
    }

    public fun check_upgraded() {
        assert!(a::MAX == 50, 0);
        assert!(DOUBLE == 100, 1);
    }
}

//# run 0x42::b::check_original

// upgrade the package, changing the constant's value

//# publish --location 0x108 --linkage 0x42=>0x108
module 0x42::a {
    public(package) const MAX: u64 = 50;
}

module 0x42::b {
    use 0x42::a;

    const DOUBLE: u64 = a::MAX * 2;

    public fun check_original() {
        assert!(a::MAX == 100, 0);
        assert!(DOUBLE == 200, 1);
    }

    public fun check_upgraded() {
        // the new version sees the new value, directly and through DOUBLE
        assert!(a::MAX == 50, 0);
        assert!(DOUBLE == 100, 1);
    }
}

// running against the upgraded version sees the new value

//# run 0x42::b::check_upgraded --linkage 0x42=>0x108

// the original version is untouched: it still returns the values it was compiled with

//# run 0x42::b::check_original
