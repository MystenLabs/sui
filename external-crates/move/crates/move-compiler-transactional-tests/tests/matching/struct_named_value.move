//# init --edition 2024.beta

//# publish
module 0x42::m {
    public struct A { x: u64 }

    fun t00(s: A): u64 {
        match (s) {
            A { x: 0 } => 1,
            A { x } => x,
        }
    }

    public fun run() {
        let a = A { x: 42 };
        assert!(a.t00() == 42);

        let b = A { x: 0 };
        assert!(b.t00() == 1);
    }
}

//# run 0x42::m::run
