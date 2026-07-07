// Tests a unit-returning macro used purely for effect -- the expansion
// produces no value to bind, exercising the statement path with no
// result store at the call site.
module A::m {
    macro fun check($cond: bool) {
        assert!($cond, 0);
    }

    public fun test(v: u64) {
        check!(v > 0);
        check!(v < 100);
    }
}
