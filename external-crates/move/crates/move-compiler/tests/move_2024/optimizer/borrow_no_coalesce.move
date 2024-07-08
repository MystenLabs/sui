module a::m;

public struct T has drop {
    x: u64,
    y: u64
}

public struct S has drop {
    a: u64,
    b: T,
}

public fun t(s: S): u64 {
    let s0 = &s;
    let b0 = &s0.b;
    let a = s0.a;
    let x = b0.x;
    let s1 = &s;
    let b1 = &s1.b;
    let n = x + a;
    let y = b1.y;
    let m = y + n;
    let m = m + m;
    m
}
