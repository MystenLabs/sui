//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Big has drop, copy {
        V1,
        V2,
        V3,
        V4,
    }

    public enum Small has drop, copy {
        A(u64),
        B,
    }

    public struct P has drop { big: Big, small: Small }

    // The all-wild `big` columns compile to binds; the later `small` column discriminates.
    public fun classify(big: Big, small: Small): u64 {
        let p = P { big, small };
        match (&p) {
            P { big: _, small: Small::A(n) } => *n,
            P { big: _, small: Small::B } => 99,
        }
    }

    public fun bigs(): vector<Big> {
        vector[Big::V1, Big::V2, Big::V3, Big::V4]
    }

    public fun a(n: u64): Small {
        Small::A(n)
    }

    public fun b(): Small {
        Small::B
    }
}

//# run
module 0x43::main {
    use 0x42::m;

    fun main() {
        let mut bigs = m::bigs();
        while (!bigs.is_empty()) {
            let big = bigs.pop_back();
            assert!(m::classify(big, m::a(5)) == 5, 0);
            assert!(m::classify(big, m::b()) == 99, 1);
        }
    }
}
