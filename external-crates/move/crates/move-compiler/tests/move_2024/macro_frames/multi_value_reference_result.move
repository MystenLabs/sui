// Tests that converting a macro's multiple mutable-reference result to
// immutable references remains in the caller's context. The generated freezes
// happen after the macro hands its result back and must not re-enter MacroBody.
module A::m {
    fun mut_refs(x: &mut u64, y: &mut u64): (&mut u64, &mut u64) {
        (x, y)
    }

    macro fun through($x: &mut u64, $y: &mut u64): (&u64, &u64) {
        mut_refs($x, $y)
    }

    public fun test(x: &mut u64, y: &mut u64): (&u64, &u64) {
        through!(x, y)
    }
}
