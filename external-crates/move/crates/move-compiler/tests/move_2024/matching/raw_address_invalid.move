module 0x42::m;

public enum E {
    V0(u64, u64)
}

public fun match_e(e: &E, rx: &u64, ry: &u64): (&u64, &u64) {
    match (e) {
        0x42::m::E::V0(x, y) => (x, y),
        0x42 => (rx, ry),
        0x42::m => (rx, ry),
    }
}
