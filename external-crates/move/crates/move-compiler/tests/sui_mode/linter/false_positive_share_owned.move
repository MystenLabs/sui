// object has store but is never created externally
module a::has_store {
    use sui::transfer;
    use sui::object::UID;

    struct Obj has key, store {
        id: UID
    }

    public fun arg_object(o: Obj) {
        let arg = o;
        transfer::public_share_object(arg);
    }
}

// object is created locally, but the analysis cannot determine that currently
module a::cannot_determine_to_be_new {
    use sui::transfer;
    use sui::object::UID;

    struct Obj has key {
        id: UID
    }

    struct X has drop {}

    fun make_obj(_: X, ctx: &mut sui::tx_context::TxContext): Obj {
        Obj { id: sui::object::new(ctx) }
    }

    public fun transfer(ctx: &mut sui::tx_context::TxContext) {
        let o = make_obj(X {}, ctx);
        transfer::transfer(o, sui::tx_context::sender(ctx));
    }

    public fun share(ctx: &mut sui::tx_context::TxContext) {
        let o = make_obj(X {}, ctx); // cannot determine this is local because of `X`
        transfer::share_object(o);
    }
}

module sui::tx_context {
    struct TxContext has drop {}
    public fun sender(_: &TxContext): address {
        @0
    }
}

module sui::object {
    struct UID has store {
        id: address,
    }
    public fun delete(_: UID) {
        abort 0
    }
    public fun new(_: &mut sui::tx_context::TxContext): UID {
        abort 0
    }
}

module sui::transfer {
    public fun transfer<T: key>(_: T, _: address) {
        abort 0
    }
    public fun share_object<T: key>(_: T) {
        abort 0
    }
    public fun public_share_object<T: key>(_: T) {
        abort 0
    }
}
