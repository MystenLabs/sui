//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum E {
        One { a: u64, b: u64, c: u64 },
        Two(u64, u64, u64)
    }

    fun do(e: E): u64 {
        match (e) {
            E::One { c: a, b: c, a: b } | E::Two(c, b, a) => a * (b + c)
        }
    }


    fun test() {
        let e_0 = E::One { a: 0, b: 1, c: 2 };
        assert!(do(e_0) == 2);
        let e_1 = E::Two(0, 1, 2);
        assert!(do(e_1) == 2);
    }
}

//# run 0x42::m::test
