//# init --edition 2024.alpha

// The synthesized constant copy disappears when the upgraded package no longer uses the
// constant cross-module; constants are not part of the upgrade-compatibility surface

//# publish
module 0x42::a {
    public(package) const MAX: u64 = 100;
}

module 0x42::b {
    use 0x42::a;

    public fun check() { assert!(a::MAX == 100, 0) }
}

//# run 0x42::b::check

//# publish --location 0x108 --linkage 0x42=>0x108
module 0x42::a {
    public(package) const MAX: u64 = 100;

    public fun local(): u64 { MAX }
}

module 0x42::b {
    public fun check() { assert!(0x42::a::local() == 100, 0) }
}

//# run 0x42::b::check --linkage 0x42=>0x108
