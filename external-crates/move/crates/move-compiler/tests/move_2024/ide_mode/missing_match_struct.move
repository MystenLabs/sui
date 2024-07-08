#[allow(ide_path_autocomplete,ide_dot_autocomplete)]
module 0x42::m {

    public struct S { x: u64 , y: u64 }

    public fun t0(s: &S): u64 {
        match (s) {
        }
    }

    public fun t1(s: &S): u64 {
        match (s) {
            S { } => 0
        }
    }
}
