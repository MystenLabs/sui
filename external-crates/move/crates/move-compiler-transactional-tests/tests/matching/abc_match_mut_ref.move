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

    public fun is_a<T>(x: &ABC<T>): bool {
        match (x) {
            ABC::A(_) => true,
            _ => false,
        }
    }

    public fun is_b<T>(x: &ABC<T>): bool {
        match (x) {
            ABC::B => true,
            _ => false,
        }
    }

    public fun is_c<T>(x: &ABC<T>): bool {
        match (x) {
            ABC::C(_) => true,
            _ => false,
        }
    }

    public fun t0(abc: &mut ABC<u64>, default: &mut u64): &mut u64 {
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
        let mut x = 43;
        let a = a().t0(&mut x);
        assert!(*a == 42, 1);
        *a = 0;
        assert!(*a == 0, 2);
        assert!(x == 43, 3);

        let c = c().t0(&mut x);
        assert!(*c == 42, 4);
        *c = 0;
        assert!(*c == 0, 5);
        assert!(x == 43, 6);

        let b = b().t0(&mut x);
        assert!(*b == 43, 7);
        *b = 0;
        assert!(*b == 0, 8);
        assert!(x == 0, 9);
    }
}
