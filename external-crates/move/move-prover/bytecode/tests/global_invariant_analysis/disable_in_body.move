module 0x1::DisableInv {
    struct R1 has key { }
    struct R2 has key { }

    fun foo(s: &signer) {
        move_to(s, R2 {});
    }
    spec foo {
        pragma disable_invariants_in_body;
    }

    spec module {
        invariant [suspendable] forall a: address where exists<R1>(a): exists<R2>(a);
    }
}
