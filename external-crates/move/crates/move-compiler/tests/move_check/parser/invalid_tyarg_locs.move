module a::m {

    struct S<T> { u: T }

    fun test00(): a::m::S<u64> {
        a<u64>::m::S { u: 0 }
    }

    fun test01(): a::m::S<u64> {
        a::m<u64>::S { u: 0 }
    }

    fun test02(): a::m::S<u64> {
        a<u64>::m<u64>::S { u: 0 }
    }

}

module 0x42::m {

    struct S<T> { u: T }

    fun test00(): S<u64> {
        0x42<u64>::m::S { u: 0 }
    }

    fun test01(): S<u64> {
        0x42::m<u64>::S { u: 0 }
    }

    fun test02(): S<u64> {
        0x42<u64>::m<u64>::S { u: 0 }
    }

}
