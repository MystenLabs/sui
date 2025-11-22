module a::m {
    // We should suggest something for Q, but not for T.
    public fun call<T>(x: &Q::r::S, y: &T): (&Q::r::S, &T) { (x, y) }
}
