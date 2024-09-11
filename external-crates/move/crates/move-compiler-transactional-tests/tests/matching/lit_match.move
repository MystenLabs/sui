//# init --edition 2024.beta

//# publish
module 0x42::m {

    public fun fib(x: u64): u64 {
        match (x) {
            0 => 1,
            1 => 1,
            x => fib(x-1) + fib(x-2),
        }
    }
}

//# run
module 0x43::main {
    use 0x42::m;
    fun main() {
        assert!(m::fib(5) == 8);
    }
}
