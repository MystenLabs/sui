//# init --edition 2024.alpha

//# publish
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    public fun t0(): u64 {
        let subject = ABC::C(0);
        match (subject) {
            ABC::C(x) => x,
            ABC::A(x) => x,
            ABC::B => 1,
        }
    }

}

//# run
module 0x42::main {

    fun main() {
        use 0x42::m::t0;
        assert!(t0() == 0, 0);
    }
}
