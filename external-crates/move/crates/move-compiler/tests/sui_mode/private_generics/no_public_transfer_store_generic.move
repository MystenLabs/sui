// tests modules cannot use transfer internal functions outside of the defining module
// even if it has store
// Note: it is not possible to make a generic type `T<...> has key, store`
// where a given instantiation`T<...>` has key but does _not_ have store

module a::m {
    use sui::transfer::{Self, Receiving};
    use sui::object::UID;

    public fun t1<T: key + store>(s: T) {
        transfer::transfer(s, @0x100);
    }

    public fun t2<T: key + store>(s: T) {
        transfer::freeze_object(s);
    }

    public fun t3<T: key + store>(s: T) {
        transfer::share_object(s);
    }

    public fun t4<T: key + store>(p: &mut UID, s: Receiving<T>): T {
        transfer::receive(p, s)
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::transfer {
    use sui::object::UID;

    struct Receiving<phantom T: key> { }

    public fun transfer<T: key>(_: T, _: address) {
        abort 0
    }

    public fun public_transfer<T: key + store>(_: T, _: address) {
        abort 0
    }

    public fun freeze_object<T: key>(_: T) {
        abort 0
    }

    public fun public_freeze_object<T: key + store>(_: T) {
        abort 0
    }

    public fun share_object<T: key>(_: T) {
        abort 0
    }

    public fun public_share_object<T: key + store>(_: T) {
        abort 0
    }

    public fun receive<T: key>(_: &mut UID, _: Receiving<T>): T {
        abort 0
    }

    public fun public_receive<T: key + store>(_: &mut UID, _: Receiving<T>): T {
        abort 0
    }
}
