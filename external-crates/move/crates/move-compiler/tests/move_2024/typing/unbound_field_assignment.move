module 0x0::m;

public struct S has copy, drop { a: u8 }
public struct T has copy, drop { a: u8, b: u8 }

fun t(s: S, t: T) {
    // an unbound field, but missing fields from the declaration
    let S { f } = s;
    f;
    let T { f, a } = t;
    f;
    a;

    // an extra unbound field
    let S { a, f } = s;
    a;
    f;
    let S { f, a } = s;
    f;
    a;
    let T { a, b, f } = t;
    a;
    b;
    f;
    let T { f, a, b } = t;
    f;
    a;
    b;
    let T { a, f, b, g } = t;
    a;
    f;
    b;
    g;

    // A small use after move error to make sure we get this far in the compiler
    let x = 0u64;
    move x;
    move x;
}
