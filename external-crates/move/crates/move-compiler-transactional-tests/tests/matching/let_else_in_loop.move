//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum ABC has drop, copy {
        A(u64),
        B,
        C(u64)
    }

    public fun a(x: u64): ABC { ABC::A(x) }
    public fun b(): ABC { ABC::B }
    public fun c(x: u64): ABC { ABC::C(x) }

    public fun sum_cs(items: vector<ABC>): u64 {
        let mut sum = 0u64;
        let len = items.length();
        let mut i = 0;
        while (i < len) {
            let item = items[i];
            i = i + 1;
            let ABC::C(x) = item else { continue };
            sum = sum + x;
        };
        sum
    }

}

//# run
module 0x43::main {

    fun main() {
        use 0x42::m::{sum_cs, a, b, c};

        let items = vector[c(1), b(), c(3), a(100), c(5)];
        assert!(sum_cs(items) == 9, 1);

        let empty: vector<0x42::m::ABC> = vector[];
        assert!(sum_cs(empty) == 0, 2);

        let all_b = vector[b(), b()];
        assert!(sum_cs(all_b) == 0, 3);
    }
}
