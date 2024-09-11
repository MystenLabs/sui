//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum ABC<T> {
        A(T),
        B,
        C(T)
    }

    public fun t0(): u64 {
        match (ABC::C(0)) {
            ABC::C(x) => x,
            ABC::A(x) => x,
            ABC::B => 1,
        }
    }
}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::t0;
        assert!(t0() == 0, 0);
    }
}
