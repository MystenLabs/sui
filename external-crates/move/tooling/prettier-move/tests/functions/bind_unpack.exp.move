// options:
// printWidth: 40

/// Module: kek
module kek::kek {
    public struct Kek {
        a: u8,
        b: u64,
    }

    public fun destroy(
        k1: Kek,
        k2: Kek,
    ) {
        let Kek { a, .. } = k1;
        let Kek { .. } = k2;

        let Slice {
            mut kek,
            prev: lprev,
            next: lnext,
            keys: mut lkeys,
            vals: mut lvals,
        } = left;
    }
}
