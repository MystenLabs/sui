// true negative cases for redundant conditional
module a::m {
    public fun t0(condition: bool) {
        if (condition) 1 else { 0 };
        if (condition) vector[] else vector[1];
    }

    public fun t1(x: u64, y: u64) {
        let _ = if (x > y) { x } else y;
    }

    // has side effects, too complex to analyze
    public fun t2(condition: &mut bool) {
        let _ = if (*condition) { *condition = false; true } else { *condition = true; false };
    }

}
