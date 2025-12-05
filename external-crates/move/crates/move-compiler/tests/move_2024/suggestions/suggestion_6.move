module a::m {
    public struct S {  }
    // We should suggest S for Q but not for T.
    public fun call<T>(x: &Q::r::S, y: &T): (&Q::r::S, &T) { (x, y) }
}
