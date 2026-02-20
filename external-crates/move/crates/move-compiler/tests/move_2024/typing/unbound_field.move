module 0x0::m;

public struct S has drop { a: u8 }
public struct T has drop { a: u8, b: u8 }

public enum E has drop {
    A {},
    B { a: u8 },
    C { a: u8, b: u8 },
}

fun t() {
    // an unbound field, but missing fields from the declaration
    S { f: false };
    T { f: false, a: 0 };
    E::B { f: false };
    E::C { a: 0, f: false };

    // an extra unbound field
    S { a: 0, f: false };
    S { f: false, a: 0 };
    T { a: 0, b: 0, f: false };
    T { f: false, a: 0, b: 0 };
    T { a: 0, f: false, b: 0 };
    E::A { f: false };
    E::B { a: 0, f: false };
    E::B { f: false, a: 0 };
    E::C { a: 0, b: 0, f: false };
    E::C { a: 0, f: false, b: 0 };
    E::C { f: false, a: 0, b: 0 };

    // A small use after move error to make sure we get this far in the compiler
    let x = 0u64;
    move x;
    move x;
}
