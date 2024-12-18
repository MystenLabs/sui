//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    public fun t0(): u64 {
        let o: Option<u64> = Option::None;
        match (o) {
            Option::Some(n) if (n == 5) => n,
            Option::None => 3,
            Option::Some(n) if (n == 3) => n,
            Option::Some(m) if (m == 2) => m,
            Option::Some(y) => y,
        }
    }
}

//# run
module 0x43::main {
    use 0x42::m;
    fun main() {
        assert!(m::t0() == 3);
    }
}
