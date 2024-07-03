module a::m {
    public fun t1(): u64 {
        let x;
        x = 0;
        move x;
        x = 1;
        x
    }

    public fun t2(cond: bool): u64 {
        let x;
        x = 0;
        if (cond) { move x; };
        x = 1;
        x
    }

    public fun t3(cond: bool): u64 {
        let x;
        if (cond) { x = 0; copy x; };
        x = 1;
        x
    }

}
