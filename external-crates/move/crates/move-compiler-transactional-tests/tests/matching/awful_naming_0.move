//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum E {
        One { a: u64, b: u64, c: u64 },
        Two { d: u64, e: u64, f: u64 },
    }

    fun test() {
        let e_0 = E::One { a: 0, b: 1, c: 2 };
        let x = match (e_0) {
            E::One { c: a, b: b, a: c } => a + c * b,
            E::Two { .. } => abort 0,
        };
        assert!(x == 2);
        let e_1 = E::Two { d: 0, e: 1, f: 2 };
        let x = match (e_1) {
            E::Two { e: d, f: e, d: f } => d * e + f,
            E::One { .. } => abort 0,
        };
        assert!(x == 2);
    }
}

//# run 0x42::m::test
