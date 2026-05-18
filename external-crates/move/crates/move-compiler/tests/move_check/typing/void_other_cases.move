module 0x2a::M {
    use std::option;

    fun f1(): u64 {
        let _o = option::some(abort 0);
        0
    }

    fun f2(): u64 {
        let _v = vector[vector[abort 0]];
        0
    }

    fun id<T>(x: T): T { x }
    fun f3(): u64 {
        let _x = id(abort 0);
        0
    }

    fun f4(cond: bool): u64 {
        let _v = vector[if (cond) abort 0 else abort 1];
        0
    }
}
