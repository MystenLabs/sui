// without a chain, we always assume a module member
module a::S {
    public struct S()
    // does not resolve to the module
    fun id(s: S): S { s }
}
// extra care given for builtins
#[allow(unused_use)]
module a::u64 {
    use a::u64; // unused
    const C: u64 = 0;
    fun new(): u64 { 0 }
}
