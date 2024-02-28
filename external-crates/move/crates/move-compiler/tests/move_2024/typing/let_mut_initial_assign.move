module a::m {
    public fun t1(): u64 {
        let x;
        x = 0;
        x
    }

    public fun t2(cond: bool): u64 {
        let x;
        if (cond) x = 1 else x = 2;
        x
    }
}
