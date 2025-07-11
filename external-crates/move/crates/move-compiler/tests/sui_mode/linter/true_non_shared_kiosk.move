module a::test_kiosk_transfer {
    use sui::transfer;
    use sui::tx_context::TxContext;
    use sui::kiosk::{Self, Kiosk};

    public fun transfer_kiosk(kiosk: Kiosk, _ctx: &mut TxContext) {
        transfer::public_transfer(kiosk, @0);
    }

    public fun create_and_transfer_kiosk(ctx: &mut TxContext) {
        let (kiosk, cap) = kiosk::new(ctx);

        transfer::public_transfer(kiosk, @0);
        transfer::public_transfer(cap, @0);
    }

    public fun freeze_kiosk(kiosk1: Kiosk, kiosk2: Kiosk) {
        transfer::public_freeze_object(kiosk1);
        transfer::public_freeze_object(kiosk2);
    }

    public fun share_kiosk(kiosk: Kiosk) {
        transfer::public_share_object(kiosk);
    }
}

module sui::kiosk {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    struct Kiosk has key, store {
        id: UID,
    }

    struct KioskCap has store, key {
        id: UID
    }

    public fun new(ctx: &mut TxContext): (Kiosk, KioskCap) {
        (Kiosk { id: object::new(ctx) }, KioskCap { id: object::new(ctx) })
    }
}

module sui::object {
    const ZERO: u64 = 0;
    struct UID has store {
        id: address,
    }
    public fun new(_: &mut sui::tx_context::TxContext): UID {
        abort ZERO
    }
}

module sui::tx_context {
    struct TxContext has drop {}
    public fun sender(_: &TxContext): address {
        @0
    }
}

module sui::transfer {
    const ZERO: u64 = 0;

    public fun public_transfer<T: key>(_: T, _: address) {
        abort ZERO
    }

    public fun public_freeze_object<T: key>(_: T) {
        abort ZERO
    }

    public fun public_share_object<T: key>(_: T) {
        abort ZERO
    }
}
