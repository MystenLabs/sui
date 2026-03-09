module 0x0::MRE {
    fun f(b: bool): u64 {
        match (b) {
            true => abort 42,
            false       0 => abort 0,
            1 => abort 1,
            _ => abort 2,
        }
    }
}
