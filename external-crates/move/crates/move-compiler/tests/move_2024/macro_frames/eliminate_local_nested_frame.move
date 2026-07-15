// Tests frame attribution when local elimination moves a value that
// itself originated in an argument expansion. `outer` passes `1` to `m`
// via `$v`, so Value(1) belongs to an Argument frame; it must remain
// attributed to that frame when substituted into a use site elsewhere.
module A::m {
    macro fun m($v: u64, $f: |u64| -> u64): u64 {
        let v = $v;
        $f(v)
    }

    macro fun outer($g: |u64| -> u64): u64 {
        m!(1, $g)
    }

    public fun test(): u64 {
        outer!(|x| x)
    }
}
