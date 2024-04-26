#[allow(unused_variable)]
module a::m {
    // macros have their own unique scopes
    macro fun foo($f: u64): u64 {
        let x = 0u64;
        $f // try to capture x
    }

    macro fun bar($f: u64): u64 {
        let x = 0u64;
        foo!($f) // try to capture x
    }

    fun t() {
        let x = vector<u64>[];
        // if we capture x, these will type check
        foo!(x);
        bar!(x);
        let x = vector<u64>[];
        foo!({ x });
        bar!({ x });
    }
}
