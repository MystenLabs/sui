module a::m {
    public struct P<T>(T) has drop;
    public struct S1 has drop { f: u64 }
    public struct S2 has drop { f: u64 }

    macro fun applyu64($f: |u64| -> u64): u64 {
        $f(0)
    }

    macro fun applyvu64($f: |vector<u64>| -> vector<u64>): vector<u64> {
        $f(vector[])
    }

    macro fun applyt<$T>($t: $T, $f: |$T| -> $T): $T {
        $f($t)
    }

    // each one of these should be rejected
    fun t() {
        applyu64!(|_: _| -> _ { 0u8 });
        applyu64!(|_: _| -> _ { b"hello" });
        applyvu64!(|_: vector<_>| -> vector<_> { b"hello" });
        applyt!(0u64, |_: vector<_>| -> vector<_> { b"hello" });
    }

}
