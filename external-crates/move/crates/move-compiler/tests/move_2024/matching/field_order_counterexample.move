module a::m;

public enum E {
    V { zero: u64, one: u64 }
}

public fun bad(e: &E) {
    match (e) {
        E::V { one: _, zero: 0 } => (),
    }
}
