//# init --edition 2024.beta

//# publish
module 0x42::m {

    public fun fib(n: u64): u64 {
        match (n) {
            0 => 0,
            1 => 1,
            n => fib(n-1) + fib(n-2)
        }
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::fib;
        assert!(fib(0) == 0, 0);
        assert!(fib(1) == 1, 1);
        assert!(fib(2) == 1, 2);
        assert!(fib(3) == 2, 3);
        assert!(fib(4) == 3, 4);
        assert!(fib(5) == 5, 5);
        assert!(fib(6) == 8, 6);
        assert!(fib(7) == 13, 7);
        assert!(fib(8) == 21, 8);
        assert!(fib(9) == 34, 9);
        assert!(fib(10) == 55, 10);
    }
}
