module A::mod0 {

    public struct S { n: u64 }

    fun t0(type: u64, enum: S, mut: bool, match: u64, for: u64): u64 {
        if (type == match) {
            type
        } else if (mut) {
            match
        } else {
            enum.n + for
        }
    }

}
