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

    public fun t2(): u64 {
        let o: Option<u64> = Option::None;
        let _y = &10;
        match (o) {
            Option::Some(n) if (_y == 5) => n,
            Option::None => 1,
            z => default(z),
        }
    }
}

//# run
module 0x43::main {
    use 0x42::m;
    fun main() {
        assert!(m::t2() == 1);
    }
}
