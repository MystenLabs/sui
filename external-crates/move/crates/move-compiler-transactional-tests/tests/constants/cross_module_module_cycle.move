//# init --edition 2024.alpha

// Constant references create no module dependency, so these modules publish and run despite
// referring to each other: 'a' and 'b' through constant definitions, 'c' and 'd' through
// function bodies

//# publish
module 0x42::a {
    public(package) const W: u64 = 1;
    public(package) const X: u64 = 0x42::b::Y + 1;
}

module 0x42::b {
    public(package) const Y: u64 = 2;
    public(package) const Z: u64 = 0x42::a::W + 1;
}

module 0x42::c {
    public(package) const C: u64 = 1;

    public fun uses_d(): u64 { 0x42::d::D }
}

module 0x42::d {
    public(package) const D: u64 = 2;

    public fun uses_c(): u64 { 0x42::c::C }

    public fun check() {
        assert!(0x42::a::X == 3, 0);
        assert!(0x42::b::Z == 2, 1);
        assert!(0x42::c::uses_d() == 2, 2);
        assert!(uses_c() == 1, 3);
    }
}

//# run 0x42::d::check
