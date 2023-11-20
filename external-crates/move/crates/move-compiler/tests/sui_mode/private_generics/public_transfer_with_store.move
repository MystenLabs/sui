// tests modules can use transfer functions outside of the defining module, if the type
// has store
module a::m {
    use sui::transfer::{Self, Receiving};
    use a::other;
    use sui::object::UID;

    public fun t(s: other::S) {
        transfer::public_transfer(s, @0x100)
    }

    public fun f(s: other::S) {
        transfer::public_freeze_object(s)
    }

    public fun s(s: other::S) {
        transfer::public_share_object(s)
    }

    public fun r(p: &mut UID, s: Receiving<other::S>): other::S {
        transfer::public_receive(p, s)
    }
}

module a::other {
    struct S has key, store {
        id: sui::object::UID,
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
