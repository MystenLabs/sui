//# init --edition 2024.alpha

//# publish
module 0x42::a {
    public(package) const MAX: u64 = 10;
}

module 0x42::b {
    public(package) const MAX: u64 = 20;
}

// module and constant names that concatenate ambiguously: 'x::A_B' and 'x_A::B'
module 0x42::x {
    public(package) const A_B: u64 = 1;
}

module 0x42::x_A {
    public(package) const B: u64 = 2;
}

module 0x42::c {
    use 0x42::a;
    use 0x42::b;
    use 0x42::x;
    use 0x42::x_A;

    const BOTH: u64 = a::MAX + b::MAX;

    public fun check() {
        assert!(a::MAX == 10, 0);
        assert!(b::MAX == 20, 1);
        assert!(BOTH == 30, 2);
        assert!(x::A_B == 1, 3);
        assert!(x_A::B == 2, 4);
        // each arm compares against its own module's value
        assert!(which(10) == 1, 5);
        assert!(which(20) == 2, 6);
        assert!(which(30) == 0, 7);
        assert!(either(10) && either(20), 8);
        assert!(!either(30), 9);
    }

    fun which(v: u64): u64 {
        match (v) {
            a::MAX => 1,
            b::MAX => 2,
            _ => 0,
        }
    }

    fun either(v: u64): bool {
        match (v) {
            a::MAX | b::MAX => true,
            _ => false,
        }
    }
}

//# run 0x42::c::check
