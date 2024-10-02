//# init --edition 2024.beta

//# publish
module 0x42::m {

    public fun fib(x: &mut u64): u64 {
        match (x) {
            0 => 1,
            1 => 1,
            x => fib(&mut (*x-1)) + fib(&mut (*x-2)),
        }
    }
}

//# run
module 0x43::main {
    use 0x42::m;
    fun main() {
        let mut n = 5;
        assert!(m::fib(&mut n) == 8);
    }
}
