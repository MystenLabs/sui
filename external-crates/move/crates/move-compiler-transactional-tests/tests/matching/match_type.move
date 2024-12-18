//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Option<T> has drop {
        None,
        Some(T)
    }

    fun foo<T: drop>(_x: T): u64 {
        10
    }

    public fun t0(): u64 {
        let o = Option::None;
        match (o) {
            Option::Some(n) => return foo(n),
            Option<u64>::None => (),
        };
        let _o = Option::Some(0);
        0
    }
}

//# run
module 0x43::main {
    use 0x42::m;
    fun main() {
        assert!(m::t0() == 0);
    }
}
