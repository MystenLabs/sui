// Tests lambda expansion frame (Lambda and Argument both parent -> MacroBody).
module A::m {
    macro fun apply($f: |u64| -> u64, $x: u64): u64 {
        $f($x)
    }

    public fun test(v: u64): u64 {
        apply!(|x| x * 2, v)
    }
}
