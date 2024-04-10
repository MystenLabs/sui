// valid locations for _, but the overall example is incorrect
module a::m {
    public struct S<T> has copy, drop { x: T }

    fun t() {
        0 as S<_>;
    }

}
