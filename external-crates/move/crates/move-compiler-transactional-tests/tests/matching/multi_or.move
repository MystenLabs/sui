//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B(T, T),
        C(T, T, T),
    }

    public fun t0(): u64 {
        test(ABC::A(0))
    }

    public fun t1(): u64 {
        test(ABC::A(1))
    }

    public fun t2(): u64 {
        test(ABC::A(2))
    }

    public fun test(subject: ABC<u64>): u64 {
        match (subject) {
            ABC::C(x, _, _) | ABC::B(_, x) | ABC::A(x) if (x == 0) => x,
            ABC::A(x) | ABC::C(x, _, _) | ABC::B(_, x)  if (x == 1) => x,
            _ => 1,
        }
    }
}

//# run
module 0x43::main {
    use 0x42::m;
    fun main() {
        assert!(m::t0() == 0);
        assert!(m::t1() == 1);
        assert!(m::t2() == 1);
    }
}
