module 0x42::m;

public enum E {
    V0(u64, u64)
}

public fun match_e(e: &E): (&u64, &u64) {
    match (e) {
        0x42::m::E::V0(x, y) => (x, y),
    }
}
