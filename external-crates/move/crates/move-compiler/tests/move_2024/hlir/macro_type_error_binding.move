module 0x42::M {
    macro fun f($x: u64): (u64, u64) {
       ($, $x)
    }
    fun test() {
        let (a, b) = f!(0);
        a;
        b;

        // A small use after move error to make sure we get this far in the compiler
        let x = 0u64;
        move x;
        move x;
    }
}
