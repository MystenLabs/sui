// Tests that forwarding a lambda through another macro preserves the source
// location of the original lambda expression on the resulting Lambda frame.
module A::m {
    macro fun apply($f: |u64| -> u64): u64 {
        $f(1)
    }

    macro fun forward($g: |u64| -> u64): u64 {
        apply!($g)
    }

    public fun test(p: u64): u64 {
        forward!(|x| x + p)
    }
}
