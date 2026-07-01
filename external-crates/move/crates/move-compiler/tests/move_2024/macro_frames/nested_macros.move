// Tests complex nesting: macro calling another macro twice with repeated argument substitutions.
module A::m {
    macro fun double($x: u64): u64 {
        $x + $x
    }

    macro fun quad($x: u64): u64 {
        double!($x) + double!($x)
    }

    public fun test(v: u64): u64 {
        quad!(v)
    }
}
