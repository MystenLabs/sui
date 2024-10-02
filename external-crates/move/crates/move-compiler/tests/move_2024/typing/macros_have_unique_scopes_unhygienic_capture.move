#[allow(unused_variable)]
module a::m {
    // macros have their own unique scopes
    macro fun foo($f: || -> u64): u64 {
        let x = 0u64;
        $f() // try to capture x
    }

    macro fun bar($f: || -> u64): u64 {
        let x = 0u64;
        foo!(|| $f()) // try to capture x
    }

    fun t() {
        // x is not in scope in either example
        foo!(|| x);
        bar!(|| x);
    }
}
