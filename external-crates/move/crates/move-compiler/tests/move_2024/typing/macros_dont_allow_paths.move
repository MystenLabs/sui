module 0x42::m {

    public struct S { x: u64 }

    macro fun bad_paths($s: S, $y: vector<u64>, $n: u64) {
        let _m = $s.x; // disallowed
        let _vs = &mut $y; // disallowed
        let _q = &$n; // disallowed
        let _q = copy $n; // disallowed
        let _q = move $n; // disallowed
    }

    fun call_macro() {
        let s = S { x: 10 };
        let n = 0;
        let vs = vector[];
        bad_paths!(s, vs, n)
    }
}
