// Tests that id!(x), called inside a lambda's if-branch, appears as
// MacroBody(id) in frame transitions. The if/else produces a
// compiler-generated assignment binding the branch result; the code
// computing the stored value must stay attributed to MacroBody(id),
// otherwise MacroBody(id) would disappear from the transitions.
module A::m {
    macro fun id($x: u64): u64 {
        $x
    }

    macro fun apply($f: |u64| -> u64): u64 {
        let arg = 1;
        $f(arg)
    }

    public fun test(p: u64): u64 {
        apply!(|x|
            if (x > p) {
                id!(x)
            } else {
                0
            }
        )
    }
}
