module 0x42::m;

public enum E {
    X { x: u64 },
    Y(u64),
    Z { z: u64 }
}

public fun test(e: &E): u64 {
    match (e) {
        E::X { x: y } | E::Y(x) | E::Z { z: x } => *x
    }
}
