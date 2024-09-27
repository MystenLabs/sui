// tests for negatives in the loop without exit lint

module a::m {
    public fun t1() {
        let i = 0;
        loop {
            if (i >= 10) break;
            i = i + 1;
        }
    }

    public fun t2() {
        let i = 0;
        loop {
            if (i >= 10) return;
            i = i + 1;
        }
    }
}
