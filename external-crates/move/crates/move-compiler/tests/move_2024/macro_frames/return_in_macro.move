// Tests `return` inside a macro body. Within a macro, `return` exits
// the expansion (not the enclosing function), compiling to a jump to
// the expansion's return label; the returned value is computed in the
// macro's frame while the store into the result binder belongs to the
// caller's frame.
module A::m {
    macro fun min($a: u64, $b: u64): u64 {
        if ($a < $b) {
            return ($a)
        };
        $b
    }

    public fun test(x: u64, y: u64): u64 {
        min!(x, y)
    }
}
