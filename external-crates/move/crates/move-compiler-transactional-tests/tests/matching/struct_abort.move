//# init --edition 2024.beta

//# publish
module 0x42::m {
    public struct S has drop { x: u64 }

    public fun from_index(s: S): u64 {
        match (s) {
            S { x: 0}  => 1,
            _ => abort 0,
        }
    }

    public fun run() {
        assert!(from_index(S { x: 0 }) == 1)
    }
}

//# run 0x42::m::run
