//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Tup<A,B,C> {
        Triple(A, B, C)
    }

    public fun match_tup(t: Tup<u64, u64, u64>): u64 {
        match (t) {
            Tup::Triple(x, y, 10) => 10 + x + y,
            Tup::Triple(x, 5, 10) => 15 + x,
            Tup::Triple(5, 5, 10) => 20,
            Tup::Triple(5, y, z) => 5 + y + z,
            Tup::Triple(5, 5, z) => 10 + z,
            Tup::Triple(x, y, z) => x + y + z,
        }
    }

    public fun run() {
        let mut i = 0;
        let mut j = 0;
        let mut k = 0;
        let end = 20;

        while (i < end) {
            while (j < end) {
                while (k < end) {
                    let t = Tup::Triple(i, j, k);
                    assert!(t.match_tup() == i + j + k);
                    k = k + 1;
                };
                j = j + 1;
                k = 0;
            };
            i = i + 1;
            j = 0;
        };
    }
}

//# run 0x42::m::run
