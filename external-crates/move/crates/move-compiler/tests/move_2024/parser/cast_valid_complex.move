module a::m {
    macro fun call($f: || -> u32) { $f(); }

    #[allow(dead_code)]
    fun weird(cond: bool, x: u64): u64 {
        // lower precedence than else, abort, return, lambda
        if (cond) 1 else x as u32;
        call!(|| 0 as u32);
        abort 0 as u64;
        return 0 as u64;
        0
    }

    public struct S has copy, drop { f: u64 }

    fun dotted(mut s: S) {
        s.f as u32;
        *&s.f as u32;
        *&mut s.f as u32;
        s[] as u32;
        *&s[] as u32;
        *&mut s[] as u32;

        S { f: 0 }.f as u32;

        s.f_val() as u32;
        *s.f_imm() as u32;
        *s.f_mut() as u32;
        s.chain().f_val() as u32;
        *s.chain().f_imm() as u32;
        *s.chain().f_mut() as u32;
        S{f:0}.f_val() as u32;
        *S{f:0}.f_imm() as u32;
        *S{f:0}.f_mut() as u32;
        S{f:0}.chain().f_val() as u32;
        *S{f:0}.chain().f_imm() as u32;
        *S{f:0}.chain().f_mut() as u32;
    }

    public fun chain(s: &mut S): &mut S { s }

    public fun f_val(s: &S): u64 { s.f }
    #[syntax(index)]
    public fun f_imm(s: &S): &u64 { &s.f }
    #[syntax(index)]
    public fun f_mut(s: &mut S): &mut u64 { &mut s.f }
}
