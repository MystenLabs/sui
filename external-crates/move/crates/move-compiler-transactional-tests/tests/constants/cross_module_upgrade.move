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

// upgrade the package, changing the constant's value: the new version is published at a new
// storage location, as on chain

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
        // the getter reads the constant of this version's 0x42::a, and DOUBLE was re-folded
        // from the new value when the upgrade was compiled
        assert!(a::MAX == 50, 0);
        assert!(DOUBLE == 100, 1);
    }
}

// running against the upgraded version sees the new value

//# run 0x42::b::check_upgraded --linkage 0x42=>0x108

// the original version is untouched: its getter and its folded constant both still hold the
// old values

//# run 0x42::b::check_original
