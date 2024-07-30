// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module token::deep {
    public struct DEEP has drop {}

    public struct ProtectedTreasury has key {
        id: UID,
    }

    public struct TreasuryCapKey has copy, drop, store {}

    public fun burn(arg0: &mut ProtectedTreasury, arg1: sui::coin::Coin<DEEP>) {
        sui::coin::burn<DEEP>(borrow_cap_mut(arg0), arg1);
    }

    public fun total_supply(arg0: &ProtectedTreasury): u64 {
        sui::coin::total_supply<DEEP>(borrow_cap(arg0))
    }

    fun borrow_cap(arg0: &ProtectedTreasury): &sui::coin::TreasuryCap<DEEP> {
        let v0 = TreasuryCapKey {};
        sui::dynamic_object_field::borrow<TreasuryCapKey, sui::coin::TreasuryCap<DEEP>>(
            &arg0.id,
            v0,
        )
    }

    fun borrow_cap_mut(arg0: &mut ProtectedTreasury): &mut sui::coin::TreasuryCap<DEEP> {
        let v0 = TreasuryCapKey {};
        sui::dynamic_object_field::borrow_mut<TreasuryCapKey, sui::coin::TreasuryCap<DEEP>>(
            &mut arg0.id,
            v0,
        )
    }

    fun create_coin(
        arg0: DEEP,
        arg1: u64,
        arg2: &mut sui::tx_context::TxContext,
    ): (ProtectedTreasury, sui::coin::Coin<DEEP>) {
        let (v0, v1) = sui::coin::create_currency<DEEP>(
            arg0,
            6,
            b"DEEP",
            b"DeepBook Token",
            b"The DEEP token secures the DeepBook protocol, the premier wholesale liquidity venue for on-chain trading.",
            std::option::some<
                sui::url::Url,
            >(sui::url::new_unsafe_from_bytes(b"https://images.deepbook.tech/icon.svg")),
            arg2,
        );
        let mut cap = v0;
        sui::transfer::public_freeze_object<sui::coin::CoinMetadata<DEEP>>(v1);
        let mut protected_treasury = ProtectedTreasury { id: sui::object::new(arg2) };

        let coin = sui::coin::mint<DEEP>(&mut cap, arg1, arg2);
        sui::dynamic_object_field::add<TreasuryCapKey, sui::coin::TreasuryCap<DEEP>>(
            &mut protected_treasury.id,
            TreasuryCapKey {},
            cap,
        );

        (protected_treasury, coin)
    }

    #[allow(lint(share_owned))]
    fun init(arg0: DEEP, arg1: &mut TxContext) {
        let (v0, v1) = create_coin(arg0, 10000000000000000, arg1);
        sui::transfer::share_object<ProtectedTreasury>(v0);
        sui::transfer::public_transfer<sui::coin::Coin<DEEP>>(v1, sui::tx_context::sender(arg1));
    }

    #[test_only]
    public fun share_treasury_for_testing(ctx: &mut sui::tx_context::TxContext) {
        let (v0, v1) = create_coin(DEEP {}, 10000000000000000, ctx);
        sui::transfer::share_object<ProtectedTreasury>(v0);
        v1.burn_for_testing();
    }
}
