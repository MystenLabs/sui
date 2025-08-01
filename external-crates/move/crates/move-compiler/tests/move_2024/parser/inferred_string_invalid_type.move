module a::m {
    public struct S { x: u64 }

    fun test() { let _x: S = "inferred stringŻ"; }
}
