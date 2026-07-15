// Tests a macro call as an argument to another macro call. The inner
// call is substituted by name, so its expansion frames nest inside the
// outer macro's Argument frame: user > double(MacroBody) > Argument >
// inc(MacroBody) > Argument. Since `double` uses `$b` twice, `inc!(v)`
// is expanded (and evaluated) twice, producing two sibling subtrees.
module A::m {
    macro fun inc($a: u64): u64 {
        $a + 1
    }

    macro fun double($b: u64): u64 {
        $b + $b
    }

    public fun test(v: u64): u64 {
        double!(inc!(v))
    }
}
