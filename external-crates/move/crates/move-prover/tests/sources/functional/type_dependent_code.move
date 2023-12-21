module 0x42::M {
    use std::signer;

    struct S<X: store> has key { x: X }

    public fun test1<X: store>(account: signer, x: X) {
        move_to<S<X>>(&account, S { x });
        move_to<S<u8>>(&account, S { x: 0 });
    }
    spec test1 {
        aborts_if exists<S<X>>(signer::address_of(account));
        aborts_if exists<S<u8>>(signer::address_of(account));

        // NOTE: besides the above aborts_if conditions, this function
        // also aborts if the type parameter `X` is instantiated with `u8`.
        // This additional abort condition is not captured by the spec.
        //
        // TODO: currently we don't even have a way to specify this additional
        // abort condition.
    }

    public fun test2<T1: store, T2: store>(account: signer, t1: T1, t2: T2) {
        move_to<S<T1>>(&account, S { x: t1 });
        move_to<S<T2>>(&account, S { x: t2 });
    }
    spec test2 {
        aborts_if exists<S<T1>>(signer::address_of(account));
        aborts_if exists<S<T2>>(signer::address_of(account));

        // NOTE: besides the above aborts_if conditions, this function
        // also aborts if type parameters `T1` and `T2` are the same.`
        // This additional abort condition is not captured by the spec.
        //
        // TODO: currently we don't even have a way to specify this additional
        // abort condition.
    }
}

module 0x42::N {
    use std::signer;

    struct S<X: store + drop> has key { x: X }

    public fun test1<X: store + drop>(account: signer, x: X) acquires S {
        move_to<S<u8>>(&account, S { x: 0 });
        let r = borrow_global_mut<S<X>>(signer::address_of(&account));
        *&mut r.x = x;
    }
    spec test1 {
        ensures global<S<u8>>(signer::address_of(account)).x == 0;

        // NOTE: the `ensures` condition might not hold when `X == u8`.
        //
        // Similar to the test cases above, we also don't have a
        // good way to specify these type equality conditions in spec.
    }

    public fun test2<T1: store + drop, T2: store + drop>(
        account: signer, t1: T1, t2: T2
    ) acquires S {
        move_to<S<T1>>(&account, S { x: t1 });
        let r = borrow_global_mut<S<T2>>(signer::address_of(&account));
        *&mut r.x = t2;
    }
    spec test2 {
        ensures global<S<T1>>(signer::address_of(account)).x == t1;

        // NOTE: the `ensures` condition might not hold when `T1 == T2`.
        //
        // Similar to the test cases above, we also don't have a
        // good way to specify these type equality conditions in spec.
        //
        // Further note that in the exp files, we see two error messages
        // on that this `ensures` condition is violated. This is expected.
        // If we take a look at the Boogie output, we will notice three
        // verification targets generated:
        // - test2<#0, #1>
        // - test2<#0, #0>
        // - test2<#1. #1>
        // The `ensures` condition does not hold in the later two cases.
    }
}
