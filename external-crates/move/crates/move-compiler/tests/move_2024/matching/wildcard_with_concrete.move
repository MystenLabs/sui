module 0x42::m;

public enum A has drop { B, C }

public fun make_b(): A { A::B }

public fun test(a: &A, b: bool): u64 {
    match (a) {
        _ if (b) => 0,
        A::B => 1,
        _ => 2,
    }
}
