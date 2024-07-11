//# init --edition 2024.alpha

//# publish
module 0x42::m {
    public enum E has drop {
        V0(u64, u64, u64)
    }

    public fun make_e(x: u64): E {
        E::V0(x, x + 1, x + 2)
    }

    public fun match_e(e: &E): (&u64, &u64, &u64) {
        match (e) {
            E::V0(x, y, z) => (x, y, z)
        }
    }
}

//# run
module 0x42::main {
    fun main() {
        let e_0 = 0x42::m::make_e(1);
        let e = &e_0;
        let (a, b, c) = 0x42::m::match_e(e);
        let (x, y, z) = 0x42::m::match_e(&e_0);
        let (q, r, s) = (*a + *x, *b + *y, *c + *z);
        let e_0_sum = q + r + s;
        let e_1 = 0x42::m::make_e(4);
        let e = &e_1;
        let (a, b, c) = 0x42::m::match_e(e);
        let (x, y, z) = 0x42::m::match_e(&e_1);
        let (t, u, v) = (*a + *x, *b + *y, *c + *z);
        let e_1_sum = t + u + v;
        let sum = e_0_sum + e_1_sum;
        assert!(sum == 42, sum);
    }
}
