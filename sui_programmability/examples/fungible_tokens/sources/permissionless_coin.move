// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This is an example of a module that would be managing a permissionless coin, that is a coin with
/// a guaranteed single generated treasury cap.
/// 
/// In a "regular" coin, it is typically expected that there is going to be only one treasury cap
/// (normally created in the initializer of the module defining the coin), but there is no way to
/// enforce this.
///
/// In order to create a (unique) treasury cap via the permissionless mint, a value representing a
/// one-time witness type needs to be used to instantiate the cap. A one-time witness type (unlike
/// a "regular" witness use to create a "regular" cap) is guaranteed to have a single instance only
/// which in turn guarantees that only one instance of the cap will be created.
/// 
/// If a "regular" type is used to attempt to create the unique treasury cap, the transaction will
/// fail.
module fungible_tokens::permissionless_mint {
    use sui::coin::{Self, TreasuryCap};
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;

    const EBadWitness: u64 = 0;

    struct UniqueTreasuryCap<phantom T> has key, store {
        id: UID,
        treasury_cap: TreasuryCap<T>,
    }

    /// Create a unique treasury cap. The first argument to this function must be of one-time
    /// witness type, otherwise the function will abort.
    public fun create_treasury_cap<T: drop>(witness: T, ctx: &mut TxContext): UniqueTreasuryCap<T> {
        assert!(object::is_one_time_witness(&witness), EBadWitness);
        UniqueTreasuryCap {
            id: object::new(ctx),
            treasury_cap: coin::create_currency(witness, ctx),
        }
    }

    /// Mint and transfer a coin using its unique treasury cap.
    public entry fun mint_and_transfer<T>(
        c: &mut UniqueTreasuryCap<T>, amount: u64, recipient: address, ctx: &mut TxContext
    ) {
        transfer::transfer(coin::mint(&mut c.treasury_cap, amount, ctx), recipient)
    }
}

module fungible_tokens::permissionless_coin {
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use fungible_tokens::permissionless_mint;

    /// One-time witness type representing a (permissionless) coin that is guaranteed to have only
    /// one treasury cap.
    struct PERMISSIONLESS_COIN has drop { }


    /// A "special" init function that creates the only ever instance of a one-time withess type.
    fun init(witness: PERMISSIONLESS_COIN, ctx: &mut TxContext) {
        transfer::transfer(
            permissionless_mint::create_treasury_cap(witness, ctx),
            tx_context::sender(ctx)
        )
    }


    #[test]
    fun test_permissionless_mint() {
        use sui::coin::Coin;
        use sui::test_scenario;
        use fungible_tokens::permissionless_mint::UniqueTreasuryCap;

        // create test addresses representing users
        let coin_admin = @0xBABE;
        let coin_owner = @0xCAFE;

        // first transaction to emulate module initialization
        let scenario = &mut test_scenario::begin(&coin_admin);
        {
            init(PERMISSIONLESS_COIN {}, test_scenario::ctx(scenario));
        };
        // second transaction executed by coin_admin to mint a coin and send it to new owner
        test_scenario::next_tx(scenario, &coin_admin);
        {
            let treasury_cap = test_scenario::take_owned<UniqueTreasuryCap<PERMISSIONLESS_COIN>>(scenario);
            permissionless_mint::mint_and_transfer(&mut treasury_cap, 42, coin_owner, test_scenario::ctx(scenario));
            test_scenario::return_owned(scenario, treasury_cap);
        };
        // third transaction executed by the owner to make sure that the coin was transferred
        test_scenario::next_tx(scenario, &coin_owner);
        {
            let coin = test_scenario::take_owned<Coin<PERMISSIONLESS_COIN>>(scenario);
            test_scenario::return_owned(scenario, coin);
        };
    }
}


module fungible_tokens::bad_coin {
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use fungible_tokens::permissionless_mint;

    struct BadCoin has drop { }

    /// A function that can be used to (potentially) crated multiple treasury caps for the same
    // coin.
    public entry fun create_treasury_cap<T>(ctx: &mut TxContext) {
        transfer::transfer(
            permissionless_mint::create_treasury_cap(BadCoin{}, ctx),
            tx_context::sender(ctx)
        )
    }


    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_permissionless_mint_fail() {
        use sui::test_scenario;
        use fungible_tokens::permissionless_mint::UniqueTreasuryCap;

        // create test addresses representing users
        let coin_admin = @0xBABE;
        let coin_owner = @0xCAFE;

        // a transaction to create two coin caps and try using them to mint coins
        let scenario = &mut test_scenario::begin(&coin_admin);
        {
            create_treasury_cap<BadCoin>(test_scenario::ctx(scenario));
            let treasury_cap1 = test_scenario::take_last_created_owned<UniqueTreasuryCap<BadCoin>>(scenario);
            // even the first minting attempt should due to treasury cap not being unique
            // (i.e., not being created with a one-time witness)
            permissionless_mint::mint_and_transfer(&mut treasury_cap1, 42, coin_owner, test_scenario::ctx(scenario));


            create_treasury_cap<BadCoin>(test_scenario::ctx(scenario));
            let treasury_cap2 = test_scenario::take_last_created_owned<UniqueTreasuryCap<BadCoin>>(scenario);
            permissionless_mint::mint_and_transfer(&mut treasury_cap2, 42, coin_owner, test_scenario::ctx(scenario));

            test_scenario::return_owned(scenario, treasury_cap1);
            test_scenario::return_owned(scenario, treasury_cap2);
        };
    }
}
