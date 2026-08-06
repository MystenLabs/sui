// Tests two things about macro frame transitions for `add1!(x) + p`
// called inside a lambda:
// 1. Parent chain: MacroBody(add1) should be nested under
//    [MacroBody(apply), Lambda], not standalone.
// 2. Scope visibility: MacroBody(add1) should appear as a distinct
//    scope in frame transitions (not absorbed into Lambda).
module A::m {
    macro fun add1($x: u64): u64 {
        $x + 1
    }

    macro fun apply($f: |u64| -> u64): u64 {
        let arg = 1;
        $f(arg)
    }

    public fun test(p: u64): u64 {
        apply!(|x|
            add1!(x) + p
        )
    }
}
