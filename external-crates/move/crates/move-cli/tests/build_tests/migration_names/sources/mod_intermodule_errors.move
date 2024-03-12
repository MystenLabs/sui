module A::mod1 {

    public struct S { n: u64 }

    public fun t0(type: u64, enum: S, mut: bool, match: u64, for: u64): u64 {
        if (type == match) {
            type
        } else if (mut) {
            match
        } else {
            enum.n + for
        }
    }

}

module A::mod2 {

    use A::mod1::t0;
    use A::mod1::S;

    public fun t1(t: u64, e: S, m: bool, m2: u64): u64 { t0(t, e, m, m2, 0) }

}
