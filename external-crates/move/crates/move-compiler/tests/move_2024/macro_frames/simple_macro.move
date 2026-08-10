// Tests all three frame types together: MacroBody, Lambda, and Argument.
module A::m {
    macro fun apply($f: |u64| -> u64, $x: u64): u64 {
        $f($x)
    }

    public fun test(v: u64): u64 {
        apply!(|x| x + 1, v)
    }
}
