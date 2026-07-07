// Tests a loop inside a macro body, with `break` carrying a value out
// of the loop. All loop control flow (entry, back edge, break) stays
// within the macro's frame; only evaluation of `$n` enters an Argument
// frame.
module A::m {
    macro fun find_sqrt_floor($n: u64): u64 {
        let mut i = 0;
        loop {
            if ((i + 1) * (i + 1) > $n) {
                break i
            };
            i = i + 1;
        }
    }

    public fun test(n: u64): u64 {
        find_sqrt_floor!(n)
    }
}
