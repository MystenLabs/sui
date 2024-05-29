module a::m {

    public struct S<T> { u: T }

    public fun make_s<T>(u: T): S<T> {
        S{ u }
    }
}

module a::n {

    fun test00(): a::m<u64>::S {
        a::m<u64>::make_s(0u64)
    }

    fun test01(): a<u64>::m::S {
        a<u64>::m::make_s(0u64)
    }

    fun test02(): a<u64>::m<u64>::S {
        a<u64>::m<u64>::make_s(0u64)
    }

    fun test03(): a::m<u64>::S<u64> {
        a::m<u64>::make_s<u64>(0u64)
    }

    fun test04(): a<u64>::m<u64>::S<u64> {
        a<u64>::m<u64>::make_s<u64>(0u64)
    }

}

module 0x42::m {

    public struct S<T> { u: T }

    public fun make_s<T>(u: T): S<T> {
        S{ u }
    }

}

module 0x42::n {

    fun test00(): 0x42::m<u64>::S {
        0x42::m<u64>::make_s(0u64)
    }

    fun test01(): 0x42<u64>::m::S {
        0x42<u64>::m::make_s(0u64)
    }

    fun test02(): 0x42<u64>::m<u64>::S {
        0x42<u64>::m<u64>::make_s(0u64)
    }

    fun test03(): 0x42::m<u64>::S<u64> {
        0x42::m<u64>::make_s<u64>(0u64)
    }

    fun test04(): 0x42<u64>::m<u64>::S<u64> {
        0x42<u64>::m<u64>::make_s<u64>(0u64)
    }

}
