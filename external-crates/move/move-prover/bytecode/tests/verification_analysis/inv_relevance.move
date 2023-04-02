module 0x2::InvRelevance {
    struct R<T: store> has key, store {
        t: T,
    }

    fun inner<T: store>(s: &signer, t: T) {
        move_to(s, R { t });
    }

    public fun outer_bool(s: &signer, t: bool) {
        inner(s, t);
    }

    public fun outer_u64(s: &signer, t: u64) {
        inner(s, t);
    }

    public fun outer_T<T: store>(s: &signer, t: T) {
        inner(s, t);
    }

    spec module {
        invariant forall a: address where exists<R<bool>>(a): global<R<bool>>(a).t;
    }
}
