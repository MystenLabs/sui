// tests modules cannot use transfer internal functions outside of the defining module

module a::m {
    use sui::transfer;
    use a::other;

    public fun t1(s: other::S) {
        transfer::transfer(s, @0x100);
    }

    public fun t2(s: other::S) {
        transfer::freeze_object(s);
    }

    public fun t3(s: other::S) {
        transfer::share_object(s);
    }
}

module a::other {
    struct S has key {
        id: sui::object::UID,
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::transfer {
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
}
