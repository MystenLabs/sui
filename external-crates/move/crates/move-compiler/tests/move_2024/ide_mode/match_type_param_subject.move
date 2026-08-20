// A match on a type-parameter subject is legal (only wildcards and binders can cover it)
// and must not ICE in IDE mode. Missing-arm reporting should suggest a wildcard when no
// default arm exists.
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
