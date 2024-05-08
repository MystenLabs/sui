module 0x42::m {

    public enum E has drop {
        A(u64),
        B(u8),
        C(u16),
    }

    fun foo(): bool {
        false
    }

    fun t0(): u64 {
        let o = &mut E::C(0);
        match (o) {
            E::A(u)  => *u,
            _ if ({*o = E::A(0); false}) => 1,
            E::B(x) => (*x as u64),
            E::C(x) => (*x as u64),
        }
    }
}
