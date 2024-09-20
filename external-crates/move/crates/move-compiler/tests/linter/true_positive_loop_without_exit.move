// tests for positives in the loop without exit lint

module a::m {
    public fun t1() {
        loop {}
    }

    const ZERO: u64 = 0;
    public fun t2() {
        loop { abort ZERO }
    }

    public fun t3() {
        loop {
            t2();
            t2();
        }
    }
}
