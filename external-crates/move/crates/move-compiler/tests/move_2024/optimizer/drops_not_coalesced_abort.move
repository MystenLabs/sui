module a::m;

public struct S {}

public fun make_s(): S { S { } }

public fun test() {
    let _s0 = make_s();
    let _s1 = make_s();
    abort 0x00F
}
