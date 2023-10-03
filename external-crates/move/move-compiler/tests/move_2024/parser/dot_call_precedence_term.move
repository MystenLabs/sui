// tests using all (potentially valid) terms on the LHS of a dot call

module a::m {

    public struct X has drop {}
    public struct Y has drop {}

    fun xeat(_: X) { abort 0 }
    fun ximm(_: &X): &Y { abort 0 }
    fun xmut(_: &mut X): &mut Y { abort 0}
    fun xval(_: X): Y { abort 0 }

    fun yimm(_: &Y): &X { abort 0 }
    fun ymut(_: &mut Y): &mut X { abort 0}
    fun yval(_: Y): X { abort 0 }

    #[allow(dead_code)]
    fun t(cond: bool): Y {
        vector [0] .length();
        vector<bool> [] .length();
        0u64.next();
        (0: u64).next();
        (0u8 as u64).next();
        { X{} }.xval();
        if (cond) X{} else { X{} }.xval();
        if (cond) X{}.xeat();
        while (cond) X{}.xeat();
        loop X{}.xeat();
        abort 0u64.next();
        Y {}
    }

}


#[defines_primitive(vector)]
module std::vector {
    #[bytecode_instruction]
    public native fun length<T>(v: &vector<T>): u64;
}

#[defines_primitive(u64)]
module std::u64 {
    public fun next(x: u64): u64 { x + 1 }
}
