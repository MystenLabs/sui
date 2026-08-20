// All-wild enum columns ahead of the discriminant compile to a bind, not a variant switch
// with the default tree cloned into every arm.
module 0x42::m;

public enum Big has drop {
    V1,
    V2,
    V3,
    V4,
}

public enum Small has drop {
    A(u64),
    B,
}

public struct P has drop { big: Big, small: Small }
public struct Q has drop { b1: Big, b2: Big, small: Small }

fun classify(p: &P): u64 {
    match (p) {
        P { big: _, small: Small::A(n) } => *n,
        P { big: _, small: Small::B } => 99,
    }
}

fun classify2(q: &Q): u64 {
    match (q) {
        Q { b1: _, b2: _, small: Small::A(n) } => *n,
        Q { b1: _, b2: _, small: _ } => 7,
    }
}
