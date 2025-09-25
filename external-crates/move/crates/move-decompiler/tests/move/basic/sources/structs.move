module basic::structs;

#[allow(unused)]
public struct S {
    x: u32,
}

#[allow(unused)]
public struct S1 {
    x: u32,
    y: u32,
    z: u32,
}

#[allow(unused)]
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

#[allow(unused)]
public struct S3 has copy, drop {
    x: u32,
    y: u32,
    z: u32,
}

#[allow(unused)]
public struct S4(u64, u64, u64) has copy, drop;

#[allow(unused_function)]
fun unpack(s: S1): u32 {
    let S1 { x: a, y: b, z: c} = s;
    a + b + c
}
