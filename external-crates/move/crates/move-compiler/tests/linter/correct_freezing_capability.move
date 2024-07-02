module 0x42::M {
    use sui::object::UID;
    use sui::transfer;

    struct Admin has key {
       id: UID
    }

    public fun freeze_cap(w: Admin) {
        transfer::public_freeze_object(w);
    }

}

module sui::object {
    struct UID has store {
        id: address,
    }
}

module sui::transfer {
    public fun public_freeze_object<T: key>(_: T) {
        abort 0
    }
}
