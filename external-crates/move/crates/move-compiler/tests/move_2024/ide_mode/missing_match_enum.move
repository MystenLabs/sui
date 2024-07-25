#[allow(ide_path_autocomplete,ide_dot_autocomplete)]
module 0x42::m {
    public enum E {
        One,
        Two(u64),
        Three { x: u64 }
    }

    public fun t0(e: &E): u64 {
        match (e) {
        }
    }

    public fun t1(e: &E): u64 {
        match (e) {
            E::Two(n) => *n
        }
    }

    public fun t2(e: &E): u64 {
        match (e) {
            E::One => 0,
            E::Two(n) => *n
        }
    }

    public fun t3(e: &E): u64 {
        match (e) {
            E::Three { x } => *x,
            E::Two(n) => *n
        }
    }
}
