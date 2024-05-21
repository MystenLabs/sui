module 0x42::m {

    fun fib(x: u64): u64 {
        match (x) {
            0 => 1,
            1 => 1,
            x => fib(x-1) + fib(x-2),
        }
    }

}
