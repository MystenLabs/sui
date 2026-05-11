//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum ABC<T> has drop {
        A(T),
        B,
        C(T)
    }

    public fun a(x: u64): ABC<u64> { ABC::A(x) }
    public fun b(): ABC<u64> { ABC::B }
    public fun c(x: u64): ABC<u64> { ABC::C(x) }

    public fun extract_c(subject: ABC<u64>): u64 {
        let ABC::C(x) = subject else { return 0 };
        x
    }

    public fun extract_a(subject: ABC<u64>): u64 {
        let ABC::A(x) = subject else { return 0 };
        x
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{extract_c, extract_a, a, b, c};

        // Pattern matches: get inner value
        assert!(extract_c(c(42)) == 42, 1);
        assert!(extract_a(a(10)) == 10, 2);

        // Pattern doesn't match: else branch returns 0
        assert!(extract_c(a(99)) == 0, 3);
        assert!(extract_c(b()) == 0, 4);
        assert!(extract_a(b()) == 0, 5);
        assert!(extract_a(c(99)) == 0, 6);
    }
}
