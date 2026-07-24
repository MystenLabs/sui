// Tests `abort` inside a macro body -- the abort and the code computing
// its value belong to the macro's frame.
module A::m {
    macro fun checked_div($a: u64, $b: u64): u64 {
        if ($b == 0) {
            abort 0
        };
        $a / $b
    }

    public fun test(x: u64, y: u64): u64 {
        checked_div!(x, y)
    }
}
