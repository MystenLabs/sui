module 0x2a::M {
    use std::option;

    // Option with divergent value
    fun f1(): u64 {
        let o = option::some(abort 0);
        0
    }

    // Nested: vector of vectors where inner is divergent
    fun f2(): u64 {
        let v = vector[vector[abort 0]];
        0
    }

    // Generic function call with divergent argument
    fun id<T>(x: T): T { x }
    fun f3(): u64 {
        let x = id(abort 0);
        0
    }

    // Divergent in both branches of if, used as type arg
    fun f4(cond: bool): u64 {
        let v = vector[if (cond) abort 0 else abort 1];
        0
    }
}
