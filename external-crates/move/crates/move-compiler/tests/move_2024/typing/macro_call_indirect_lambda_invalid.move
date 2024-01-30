module a::m {
    macro fun apply($f: |u64| -> u64, $x: u64): u64 {
        $f($x)
    }

    fun t() {
        let f = |x| x + 1;
        let x = apply!(f, 1);
    }

    fun t2() {
        let f: |u64| -> u64;
        let x = apply!(f, 1);
    }

    fun t3() {
        let x = apply!((0: |u64| -> u64), 1);
    }

    fun t4() {
        let x = apply!(|x| x, |x| x);
    }
}
