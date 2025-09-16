module basic::simple;

public struct S {
    x: u32,
}

public struct S2 {
    a: u32,
    b: u32,
    c: u32,
    d: u32,
    e: u32,
    f: u32,
    g: u32,
    h: u32,
    i: u32,
    j: u32,
    k: u32,
    l: u32,
    m: u32,
    n: u32,
}


public enum E {
    X(),
    Y(u32),
}

public fun f(s: &S): &u32 {
    &s.x
}


