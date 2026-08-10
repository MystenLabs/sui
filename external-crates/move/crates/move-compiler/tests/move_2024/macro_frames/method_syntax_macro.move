// Tests a macro invoked with method syntax. The receiver is passed as
// the first (by-name) argument, so it gets an Argument frame just like
// a positional argument would.
module A::m {
    public struct S has drop { v: u64 }

    macro fun bump($s: &S, $by: u64): u64 {
        // macro parameters cannot appear in paths, so bind before field access
        let s = $s;
        s.v + $by
    }

    public fun test(s: S): u64 {
        s.bump!(1)
    }
}
