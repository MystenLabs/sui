//# init --edition 2024.beta

//# publish
module 0x42::m {
    public fun test(value: bool): u64 {
        match (value) {
            true => abort 0,
            false => 0,
        }
    }

    public fun run() {
        assert!(test(false) == 0)
    }
}

//# run 0x42::m::run
