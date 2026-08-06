// Tests a macro call in a `while` condition -- the frame transitions
// repeat on every loop iteration, including after the back edge.
module A::m {
    macro fun lt($a: u64, $b: u64): bool {
        $a < $b
    }

    public fun test(): u64 {
        let mut i = 0;
        while (lt!(i, 10)) {
            i = i + 1;
        };
        i
    }
}
