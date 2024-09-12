//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Threes<T> has drop {
        Zero,
        One(T),
        Two(T, T),
        Three(T, T, T)
    }

    public fun t0(): u64 {
        match (Threes::One(0)) {
            Threes::Three(x, _, _) => x,
            Threes::One(x) => x,
            Threes::Two(_, _) => 1,
            Threes::Zero => 64,
        }
    }

}

//# run
module 0x43::main {
    use 0x42::m;
    fun main() {
        assert!(m::t0() == 0);
    }
}
