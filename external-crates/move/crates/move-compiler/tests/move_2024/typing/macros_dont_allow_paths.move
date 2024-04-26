module 0x42::m {

    public struct S has drop { x: u64 }

    macro fun bad_paths($s: S, $y: vector<u64>, $n: u64): (S, vector<u64>, u64) {
        let _m = $s.x; // disallowed
        let _vs = &mut $y; // disallowed
        let _q = &$n; // disallowed
        let _q = copy $n; // disallowed
        let _q = move $n; // disallowed
        ($s, $y, $n)
    }

    fun call_macro() {
        let s = S { x: 10 };
        let n = 0;
        let vs = vector[];
        let (_, _, _) = bad_paths!(s, vs, n);
    }
}
