module a::m {
    public struct S<phantom $T>()
    fun foo<T>($x: u64, $f: |u64|) {
        $f($x)
    }
}
