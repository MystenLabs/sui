// tests using all (syntactically valid, but typing invalid) terms on the LHS of a dot call

module a::m {

    public struct X has drop {}
    public struct Y has drop {}

    fun ximm(_: &X): &Y { abort 0 }
    fun xmut(_: &mut X): &mut Y { abort 0}
    fun xval(_: X): Y { abort 0 }

    fun yimm(_: &Y): &X { abort 0 }
    fun ymut(_: &mut Y): &mut X { abort 0}
    fun yval(_: Y): X { abort 0 }

    fun t(cond: bool) {
        loop {
            break.xval();
            continue.xval();
        };
        vector [] .length();
        (X{}, Y{}).xval();
        0.next();
        while (cond) { }.xeat();
        return { X{} }.xval();
        return.xval();
        abort { 0 } .length();
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
