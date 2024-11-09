//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    public fun a(): ABC<u64> {
        ABC::A(42)
    }

    public fun b(): ABC<u64> {
        ABC::B
    }

    public fun c(): ABC<u64> {
        ABC::C(42)
    }

    public fun t0(abc: &ABC<u64>, default: &u64): &u64 {
        match (abc) {
            ABC::C(x) => x,
            ABC::A(x) => x,
            ABC::B => default,
        }
    }
}

//# run
module 0x43::main {
    use 0x42::m::{a, b, c};
    fun main() {
        let x = 43;
        let a = a().t0(&x);
        assert!(*a == 42, 1);
        assert!(x == 43, 2);

        let c = c().t0(&x);
        assert!(*c == 42, 3);
        assert!(x == 43, 4);

        let b = b().t0(&x);
        assert!(*b == 43, 5);
        assert!(x == 43, 6);
    }
}
