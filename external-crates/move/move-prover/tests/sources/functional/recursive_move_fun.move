module 0x1::test {
    public fun pow(n: u64, e: u64): u64 {
        if (e == 0) {
            1
        } else {
            n * pow(n, e - 1)
        }
    }
    spec pow {
        // NOTE: opaque declaration is necessary for a recursive move function
        pragma opaque;
        ensures result == spec_pow(n, e);
    }

    spec fun spec_pow(n: num, e: num): num {
        if (e == 0) { 1 } else { n * spec_pow(n, e - 1) }
    }

    public fun foo_1() {
        foo_2();
    }

    fun foo_2() {
        foo_3();
        foo_2();
    }
    spec foo_2 {
        pragma opaque;
    }

    fun foo_3() {
    }

}
