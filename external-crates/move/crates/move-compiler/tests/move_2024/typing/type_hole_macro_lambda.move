module a::m {
    public struct P<T>(T) has drop;
    public struct S1 has drop { f: u64 }
    public struct S2 has drop { f: u64 }

    macro fun apply($vs: _, $f: _): _ {
        $f($vs)
    }

    macro fun apply_either($cond: bool, $a: _, $b: _, $f: _): _ {
        if ($cond) $f($a) else $f($b)
    }

    macro fun apply_either2($cond: bool, $a: _, $b: _, $f: |_| -> _): _ {
        if ($cond) $f($a) else $f($b)
    }

    fun t() {
        apply!(1, |x| vector[x]);
        apply!(P(1), |P(x)| vector[x]);
        apply_either!(true, S1 { f: 0 }, S2 { f: 0 }, |s| vector[s.f]);
        apply_either2!(true, S1 { f: 0 }, S2 { f: 0 }, |s| vector[s.f]);
    }

}
