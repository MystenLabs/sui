//# init --edition 2024.beta

//# publish
module 0x42::m {
    public fun from_index(index: u64): u64 {
        match (index) {
            0 => 1,
            1 => 2,
            2 => 3,
            _ => abort 0,
        }
    }

    public fun run() {
        assert!(from_index(2) == 3)
    }
}

//# run 0x42::m::run
