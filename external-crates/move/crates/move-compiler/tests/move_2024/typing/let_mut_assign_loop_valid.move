// these assignments, while in a loop are not "mutations"
module a::m {
    public fun t0(cond: bool) {
        while (cond) { let a = 0; a; };
    }

    public fun t1() {
        loop { let b = 0; b; }
    }

    public fun t2() {
        let x;
        loop { x = 0; x; break };
    }
}
