// Tests two independent macro calls in sequence within the same function.
module A::m {
    macro fun add_one($x: u64): u64 {
        $x + 1
    }

    macro fun double($x: u64): u64 {
        $x + $x
    }

    public fun test(v: u64): u64 {
        add_one!(v) + double!(v)
    }
}
