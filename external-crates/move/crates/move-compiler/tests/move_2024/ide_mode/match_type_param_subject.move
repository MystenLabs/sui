#[allow(ide_path_autocomplete,ide_dot_autocomplete)]
module 0x42::m {
    public fun exhaustive<T>(t: T): T {
        match (t) {
            x => x,
        }
    }

    public fun exhaustive_ref<T>(t: &T): u64 {
        match (t) {
            _ => 1,
        }
    }

    public fun guards_only<T: drop>(t: T, flag: bool): u64 {
        match (t) {
            _ if (flag) => 0,
        }
    }
}
