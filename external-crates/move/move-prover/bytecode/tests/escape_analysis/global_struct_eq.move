module 0x1::StructEq {

    struct S has key { f: u64 }

    invariant forall a: address: global<S>(a).f == 10;

    public fun new(): S {
        S { f: 10 }
    }

    public fun publish(account: &signer, s: S) {
        move_to(account, s)
    }

    // should complain
    public fun leak_f(s: &mut S): &mut u64 {
        &mut s.f
    }
}
