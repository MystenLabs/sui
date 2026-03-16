// Tests that unused `let mut` in macro bodies is detected at the definition site.
// This is a pre-expansion analysis: it catches macro-author errors without relying
// on call-site expansion.

module a::m {
    // x is never mutated in the macro body — should warn.
    macro fun unused_mut(): u64 {
        let mut x = 5u64;
        x + 1
    }

    // y IS mutated via assignment — should NOT warn.
    macro fun used_mut(): u64 {
        let mut y = 0u64;
        y = 42u64;
        y
    }

    // z is mutated via field mutation — should NOT warn.
    // (This test can't run without a struct, but the check is structural.)
    public struct S has copy, drop { value: u64 }

    macro fun field_mut(): S {
        let mut s = S { value: 0 };
        s.value = 42;
        s
    }

    // v is mutated via method call — should NOT warn (conservative).
    macro fun method_mut(): vector<u64> {
        let mut v = vector[];
        v.push_back(1u64);
        v
    }

    // Macro parameter: $x should never produce unused_let_mut.
    macro fun with_param($x: u64): u64 {
        $x + 1
    }

    fun call_them(): u64 {
        unused_mut!() + used_mut!() + field_mut!().value + with_param!(1)
    }
}
