//# init --edition 2024.alpha

//# publish
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    fun fib(x: u64): u64 {
        match (x) {
            0 => 1,
            1 => 1,
            x => fib(x-1) + fib(x-2),
        }
    }

    fun test() {
        assert!(fib(5) == 8, 0);
    }

}

//# run 0x42::m::test
