module a::m {

    public struct S has copy, drop { f: u64 }

    fun dotted(cond: bool, mut s: S) {
        1 + s.f as u32;
        1 + S { f: 0 }.f as u32;
        *if (cond) { &0 } else { &mut 0 } as u32;
        *if (cond) { &s } else {&mut s}.f_imm() as u32;
        // This case still do not work
        (*if (cond) { &0 } else { &mut 0 } as u32);
    }

    public fun f_imm(s: &S): &u64 { &s.f }
}
