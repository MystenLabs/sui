//# init --edition development

//# publish
module 0x42::m {

    public fun fib(n: u64): u64 {
        match (n) {
            0 => 1,
            1 => 1,
            n => fib(n-1) + fib(n-2)
        }
    }

}

//# run
module 0x42::main {

    fun main() {
        use 0x42::m::fib;
        assert!(fib(0) == 1, 0);
        assert!(fib(1) == 1, 1);
        assert!(fib(2) == 2, 2);
        assert!(fib(3) == 3, 3);
        assert!(fib(4) == 5, 4);
        assert!(fib(5) == 8, 5);
        assert!(fib(6) == 13, 6);
        assert!(fib(7) == 21, 7);
        assert!(fib(8) == 34, 8);
        assert!(fib(9) == 55, 9);
        assert!(fib(10) == 89, 10);
    }
}
