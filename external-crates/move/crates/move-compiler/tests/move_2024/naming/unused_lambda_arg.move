module a::m {
    public struct Point { x: u64, y: u64 } has copy, drop;
    macro fun p($f: |Point| -> u64): u64 {
        $f(Point { x: 1, y: 2 })
    }

    macro fun r($f: |&Point| -> u64): u64 {
        $f(&Point { x: 1, y: 2 })
    }

    fun t() {
        p!(|p| 0);
        p!(|Point { x, y }| x);
        r!(|p| 0);
        r!(|Point { x, y }| *x);
    }
}
