module 0x42::m {
    public struct X<T> { t: T }
    public struct Y<T> { t: T }
}

module 0x42::n {
    public struct A { y: 0x42::m::X::Y }
    public struct B { x: 0x42::m::X::X }

    fun foo(): 0x42::m::X<042::m::Y::Y> {
        0x42::m::X<u64>::X { t: abort 0 }
    }
}
