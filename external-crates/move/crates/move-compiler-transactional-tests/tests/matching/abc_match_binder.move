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

    public fun t0(x: ABC<u64>): ABC<u64> {
        match (x) {
            ABC::C(c) => ABC::C(c),
            ABC::A(a) => ABC::A(a),
            y => y,
        }
    }

}

//# run
module 0x43::main {
    use 0x42::m::{a, b, c};
    fun main() {
        assert!(a().t0().is_a(), 0);
        assert!(b().t0().is_b(), 1);
        assert!(c().t0().is_c(), 2);
    }
}
