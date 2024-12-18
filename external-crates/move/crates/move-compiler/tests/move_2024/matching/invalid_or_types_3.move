module 0x42::m;

public enum E {
    X { x: u64 },
    Y(u32)
}

public fun test(e: &E): u64 {
    match (e) {
        E::X { x: y } | E::Y(y) => *y
    }
}
