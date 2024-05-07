module a::m {
    macro fun foo($x: u64): u64 {
        $x + $x
    }

    fun t(cond: bool) {
        // mostly making sure the error doesn't say this is a lambda
        foo!('a: {
            if (cond) return'a vector[];
            0
        });
        foo!('a: {
            if (cond) return'a 0;
            vector[]
        });
    }
}
