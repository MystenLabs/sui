module a::m {
    public fun t0(cond: bool) {
        let a;
        while (cond) { a = 0; a; };
    }

    public fun t1() {
        let b;
        loop { b = 0; b; }
    }

    public fun t2(cond: bool): u64 {
        let x;
        while (cond) { x = 1; x; };
        x = 1;
        x
    }

    public fun t3(): u64 {
        let x;
        loop {
            x = 0;
            move x;
        }
    }
}
