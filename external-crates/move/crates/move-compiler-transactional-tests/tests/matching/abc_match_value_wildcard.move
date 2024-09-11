//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    public fun t0(): u64 {
        match (ABC::C(0)) {
            ABC::C(x) => x,
            _ => 1,
        }
    }

}

//# run
module 0x43::main {
    use 0x42::m::t0;
    fun main() {
        assert!(t0() == 0, 0);
    }
}
