module 0x42::m {

    fun fib(x: &mut u64): u64 {
        match (x) {
            0 => 1,
            1 => 1,
            x => fib(&mut (*x-1)) + fib(&mut (*x-2)),
        }
    }

}
