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
        test(Option::None)
    }

    public fun t1(): u64 {
        test(Option::Some(1))
    }

    fun test(o: Option<u64>): u64 {
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
        assert!(m::t0() == 1);
        assert!(m::t1() == 0);
    }
}
