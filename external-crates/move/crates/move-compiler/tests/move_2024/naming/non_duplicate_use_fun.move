// these cases look like they might cause duplicate use funs, but they
// do not for some reasons (see comments on each case)

module a::vals {
    public struct Num has copy, drop {
        num: u64,
    }
    public struct Cond has copy, drop {
        cond: bool,
    }

    public fun copy_num(n: &Num): u64 {
        n.num
    }

    public fun copy_cond(c: &Cond): bool {
        c.cond
    }

    public fun code(): Num {
        Num { num: 0 }
    }
}

module a::malicious {
    public fun num(num: &a::vals::Num): u64 {
        abort num.copy_num()
    }
}

module b::example {
    use a::vals::{Self, Num, Cond};

    public fun tnum(p: &Num): u64 {
        // does not create a conflict since a::malicious::num does not
        // define a::vals::Num
        use fun vals::copy_num as Num.num;
        use a::malicious::num;
        let v = p.num();
        if (v == 0) num(p)
        else v
    }

    public fun tcond(p: &Num, cond: &Cond): u64 {
        // does not create a conflict since a::vals::copy_cond does not
        // take a a::vals::Num
        use fun vals::copy_num as Num.f;
        use a::vals::copy_cond as f;
        if (f(cond)) p.f()
        else 0
    }

    public fun tcode(p: &Num): u64 {
        // does not create a conflict since a::vals::code does not
        // take a a::vals::Num
        use fun vals::copy_num as Num.code;
        use a::vals::code;
        let v = p.code();
        if (v == 0) a::malicious::num(&code())
        else v
    }
}
