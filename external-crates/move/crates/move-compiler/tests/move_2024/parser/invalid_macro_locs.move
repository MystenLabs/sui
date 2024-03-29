module a::m {

    public struct S<T> { u: T }

    public macro fun make_s<$T>($u: $T): S<$T> {
        S{ u: $u }
    }
}

module a::n {

    fun test00(): a::m::S<u64> {
        a!::m::make_s<u64>(0u64)
    }

    fun test01(): a::m::S<u64> {
        a::m!::make_s<u64>(0u64)
    }

    fun test02(): a::m::S<u64> {
        a!::m!::make_s<u64>(0u64)
    }

    fun test03(): a::m::S<u64> {
        a::m!::make_s!<u64>(0u64)
    }

    fun test04(): a::m::S<u64> {
        a!::m!::make_s!<u64>(0u64)
    }

}

module 0x42::m {

    public struct S<T> { u: T }

    public macro fun make_s<$T>($u: $T): S<$T> {
        S{ u: $u }
    }
}

module 0x42::n {

    fun test00(): 0x42::m::S<u64> {
        0x42!::m::make_s<u64>(0u64)
    }

    fun test01(): 0x42::m::S<u64> {
        0x42::m!::make_s<u64>(0u64)
    }

    fun test02(): 0x42::m::S<u64> {
        0x42!::m!::make_s<u64>(0u64)
    }

    fun test03(): 0x42::m::S<u64> {
        0x42::m!::make_s!<u64>(0u64)
    }

    fun test04(): 0x42::m::S<u64> {
        0x42!::m!::make_s!<u64>(0u64)
    }

}
