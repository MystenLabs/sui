//# init --edition 2024.beta

//# publish
module 0x42::m {
    public fun test(value: bool): u64 {
        match (value) {
            true => match (value) { true => abort 0, false => abort 0 },
            false => match (value) { true => abort 0, false => 1 },
        }
    }

    public fun run() {
        assert!(test(false) == 1)
    }
}

//# run 0x42::m::run
