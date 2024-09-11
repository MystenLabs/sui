//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun default<T: drop>(_o: Option<T>): u64 {
        0
    }

    public fun t0(): u64 {
        let o: Option<u64> = Option::None;
        match (o) {
            Option::None => 3,
            z => default(z),
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
