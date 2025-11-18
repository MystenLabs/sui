// these assignments, while in a loop are not "mutations"
module a::m {
    public fun t0(cond: bool) {
        while (cond) { let a = 0u64; a; };
    }

    public fun t1() {
        loop { let b = 0u64; b; }
    }

    public fun t2() {
        let x;
        loop { x = 0u64; x; break };
    }
}
