// implicitly copy constants result in a warning
module a::m {
    const C: u64 = 0;
    const BYTES: vector<u8> = b"hello";

    fun check() {
        C.next();
        BYTES.length();
        BYTES.push_back(0);

    }
}

#[defines_primitive(vector)]
module std::vector {
    #[bytecode_instruction]
    public native fun length<T>(v: &vector<T>): u64;

    #[bytecode_instruction]
    native public fun push_back<Element>(v: &mut vector<Element>, e: Element);
}

#[defines_primitive(u64)]
module std::u64 {
    public fun next(x: u64): u64 { x + 1 }
}
