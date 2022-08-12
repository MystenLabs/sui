// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module representing an example implementation for private coins. 
///
/// To implement any of the methods, module defining the type for the currency
/// is expected to implement the main set of methods such as `borrow()`,
/// `borrow_mut()` and `zero()`.

// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
module fungible_tokens::private_coin {
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::vector;
    use sui::object::{Self, UID};
    use fungible_tokens::private_balance::{Self, PrivateBalance, Supply};
    use sui::crypto::{RistrettoPoint};

    /// A coin of type `T` worth `value`. Transferable and storable
    struct PrivateCoin<phantom T> has key, store {
        id: UID,
        balance: PrivateBalance<T>
    }

    /// Capability allowing the bearer to mint and burn
    /// coins of type `T`. Transferable
    struct TreasuryCap<phantom T> has key, store {
        id: UID,
        total_supply: Supply<T>
    }

    // === Supply <-> TreasuryCap morphing and accessors  ===

    /// Return the total number of `T`'s in circulation.
    public fun total_supply<T>(cap: &TreasuryCap<T>): u64 {
        private_balance::supply_value(&cap.total_supply)
    }

    /// Wrap a `Supply` into a transferable `TreasuryCap`.
    public fun treasury_from_supply<T>(total_supply: Supply<T>, ctx: &mut TxContext): TreasuryCap<T> {
        TreasuryCap { id: object::new(ctx), total_supply }
    }

    /// Unwrap `TreasuryCap` getting the `Supply`.
    public fun treasury_into_supply<T>(treasury: TreasuryCap<T>): Supply<T> {
        let TreasuryCap { id, total_supply } = treasury;
        object::delete(id);
        total_supply
    }

    /// Get immutable reference to the treasury's `Supply`.
    public fun supply<T>(treasury: &mut TreasuryCap<T>): &Supply<T> {
        &treasury.total_supply
    }

    /// Get mutable reference to the treasury's `Supply`.
    public fun supply_mut<T>(treasury: &mut TreasuryCap<T>): &mut Supply<T> {
        &mut treasury.total_supply
    }

    // === Balance <-> Coin accessors and type morphing ===

    /// Get immutable reference to the balance of a coin.
    public fun balance<T>(coin: &PrivateCoin<T>): &PrivateBalance<T> {
        &coin.balance
    }

    /// Get a mutable reference to the balance of a coin.
    public fun balance_mut<T>(coin: &mut PrivateCoin<T>): &mut PrivateBalance<T> {
        &mut coin.balance
    }

    /// Wrap a balance into a Coin to make it transferable.
    public fun from_balance<T>(balance: PrivateBalance<T>, ctx: &mut TxContext): PrivateCoin<T> {
        PrivateCoin { id: object::new(ctx), balance }
    }

    /// Destruct a Coin wrapper and keep the balance.
    public fun into_balance<T>(coin: PrivateCoin<T>): PrivateBalance<T> {
        let PrivateCoin { id, balance } = coin;
        object::delete(id);
        balance
    }

    /// Take a `Coin` worth of `value` from `Balance`.
    /// Aborts if `value > balance.value`
    public fun take<T>(
        balance: &mut PrivateBalance<T>, new_commitment: RistrettoPoint, proof: vector<u8>, ctx: &mut TxContext,
    ): PrivateCoin<T> {
        PrivateCoin {
            id: object::new(ctx),
            balance: private_balance::split(balance, new_commitment, proof)
        }
    }

    /// Put a `Coin<T>` to the `Balance<T>`.
    public fun put<T>(balance: &mut PrivateBalance<T>, coin: PrivateCoin<T>) {
        private_balance::join(balance, into_balance(coin));
    }

    // === Functionality for Coin<T> holders ===

    /// Send `c` to `recipient`
    public entry fun transfer<T>(c: PrivateCoin<T>, recipient: address) {
        transfer::transfer(c, recipient)
    }

    /// Transfer `c` to the sender of the current transaction
    public fun keep<T>(c: PrivateCoin<T>, ctx: &TxContext) {
        transfer(c, tx_context::sender(ctx))
    }

    /// Consume the coin `c` and add its value to `self`.
    /// Aborts if `c.value + self.value > U64_MAX`
    public entry fun join<T>(self: &mut PrivateCoin<T>, c: PrivateCoin<T>) {
        let PrivateCoin { id, balance } = c;
        object::delete(id);
        private_balance::join(&mut self.balance, balance);
    }

    /// Join everything in `coins` with `self`
    public entry fun join_vec<T>(self: &mut PrivateCoin<T>, coins: vector<PrivateCoin<T>>) {
        let i = 0;
        let len = vector::length(&coins);
        while (i < len) {
            let coin = vector::remove(&mut coins, i);
            join(self, coin);
            i = i + 1
        };
        // safe because we've drained the vector
        vector::destroy_empty(coins)
    }

    // === Registering new coin types and managing the coin supply ===

    /// Make any Coin with a zero value. Useful for placeholding
    /// bids/payments or preemptively making empty balances.
    public fun zero<T>(ctx: &mut TxContext): PrivateCoin<T> {
        PrivateCoin { id: object::new(ctx), balance: private_balance::zero() }
    }

    /// Create a new currency type `T` as and return the `TreasuryCap`
    /// for `T` to the caller.
    /// NOTE: It is the caller's responsibility to ensure that
    /// `create_currency` can only be invoked once (e.g., by calling it from a
    /// module initializer with a `witness` object that can only be created
    /// in the initializer).
    public fun create_currency<T: drop>(
        witness: T,
        ctx: &mut TxContext
    ): TreasuryCap<T> {
        TreasuryCap {
            id: object::new(ctx),
            total_supply: private_balance::create_supply(witness)
        }
    }

    /// Create a coin worth `value`. and increase the total supply
    /// in `cap` accordingly.
    public fun mint<T>(
        cap: &mut TreasuryCap<T>, value: u64, blinding_factor: vector<u8>, ctx: &mut TxContext,
    ): PrivateCoin<T> {
        PrivateCoin {
            id: object::new(ctx),
            balance: private_balance::increase_supply(&mut cap.total_supply, value, blinding_factor)
        }
    }

    /// Mint some amount of T as a `Balance` and increase the total
    /// supply in `cap` accordingly.
    /// Aborts if `value` + `cap.total_supply` >= U64_MAX
    public fun mint_balance<T>(
        cap: &mut TreasuryCap<T>, value: u64, blinding_factor: vector<u8>
    ): PrivateBalance<T> {
        private_balance::increase_supply(&mut cap.total_supply, value, blinding_factor)
    }

    /// Give away the treasury cap to `recipient`
    public fun transfer_cap<T>(c: TreasuryCap<T>, recipient: address) {
        transfer::transfer(c, recipient)
    }

    // === Entrypoints ===

    /// Mint `amount` of `Coin` and send it to `recipient`. Invokes `mint()`.
    public entry fun mint_and_transfer<T>(
        c: &mut TreasuryCap<T>, amount: u64, blinding_factor: vector<u8>, recipient: address, ctx: &mut TxContext
    ) {
        transfer::transfer(mint(c, amount, blinding_factor, ctx), recipient)
    }

    /// Send `amount` units of `c` to `recipient
    /// Aborts with `EVALUE` if `amount` is greater than or equal to `amount`
    public entry fun split_and_transfer<T>(
        c: &mut PrivateCoin<T>, new_commitment: RistrettoPoint, proof: vector<u8>, recipient: address, ctx: &mut TxContext
    ) {
        transfer::transfer(take(&mut c.balance, new_commitment, proof, ctx), recipient)
    }

    /// Split coin `self` to two coins, one with balance `split_amount`,
    /// and the remaining balance is left is `self`.
    public entry fun split<T>(self: &mut PrivateCoin<T>, new_commitment: RistrettoPoint, proof: vector<u8>, ctx: &mut TxContext) {
        transfer::transfer(
            take(&mut self.balance, new_commitment, proof, ctx),
            tx_context::sender(ctx)
        )
    }

    // === Test-only code ===

    #[test_only]
    /// Mint coins of any type for (obviously!) testing purposes only
    public fun mint_for_testing<T>(value: u64, ctx: &mut TxContext): PrivateCoin<T> {
        PrivateCoin { id: object::new(ctx), balance: private_balance::create_for_testing(value) }
    }

    #[test_only]
    /// Destroy a `Coin` with any value in it for testing purposes.
    public fun destroy_for_testing<T>(self: PrivateCoin<T>): RistrettoPoint {
        let PrivateCoin { id, balance } = self;
        object::delete(id);
        private_balance::destroy_for_testing(balance)
    }
}

#[test_only]
module fungible_tokens::test_private_coin {
    use sui::test_scenario::{Self, ctx};
    use fungible_tokens::private_coin::{Self};
    use fungible_tokens::private_balance::{Self};
    use sui::sui::SUI;
    use sui::locked_coin::LockedCoin;
    use sui::tx_context;
    use sui::locked_coin;
    use sui::crypto::Self;

    const TEST_SENDER_ADDR: address = @0xA11CE;
    const TEST_RECIPIENT_ADDR: address = @0xB0B;

    #[test]
    fun test_balance() {
        let balance = private_balance::zero<SUI>();
        let another = private_balance::create_for_testing(1000);

        private_balance::join(&mut balance, another);

        let point = crypto::create_pedersen_commitment(
            crypto::big_scalar_from_u64(1),
            crypto::big_scalar_from_u64(2)
        );
        let balance1 = private_balance::split(&mut balance, 3, vector[0]);

        balance::destroy_zero(balance);

        assert!(balance::value(&balance1) == 333, 1);
        assert!(balance::value(&balance2) == 333, 2);
        assert!(balance::value(&balance3) == 334, 3);

        balance::destroy_for_testing(balance1);
        balance::destroy_for_testing(balance2);
        balance::destroy_for_testing(balance3);
    }

    // #[test]
    // public entry fun test_locked_coin_valid() {
    //     let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
    //     let ctx = test_scenario::ctx(scenario);
    //     let coin = private_coin::mint_for_testing<SUI>(42, ctx);

    //     test_scenario::next_tx(scenario, &TEST_SENDER_ADDR);

    //     // Advance the epoch by 2.
    //     test_scenario::next_epoch(scenario);
    //     test_scenario::next_epoch(scenario);
    //     assert!(tx_context::epoch(test_scenario::ctx(scenario)) == 2, 1);

    //     test_scenario::next_tx(scenario, &TEST_RECIPIENT_ADDR);
    //     let locked_coin = test_scenario::take_owned<LockedCoin<SUI>>(scenario);
    //     // The unlock should go through since epoch requirement is met.
    //     locked_coin::unlock_coin(locked_coin, test_scenario::ctx(scenario));

    //     test_scenario::next_tx(scenario, &TEST_RECIPIENT_ADDR);
    //     let unlocked_coin = test_scenario::take_owned<Coin<SUI>>(scenario);
    //     assert!(coin::value(&unlocked_coin) == 42, 2);
    //     coin::destroy_for_testing(unlocked_coin);
    // }

    // #[test]
    // #[expected_failure(abort_code = 1)]
    // public entry fun test_locked_coin_invalid() {
    //     let scenario = &mut test_scenario::begin(&TEST_SENDER_ADDR);
    //     let ctx = test_scenario::ctx(scenario);
    //     let coin = coin::mint_for_testing<SUI>(42, ctx);

    //     test_scenario::next_tx(scenario, &TEST_SENDER_ADDR);
    //     // Lock up the coin until epoch 2.
    //     locked_coin::lock_coin(coin, TEST_RECIPIENT_ADDR, 2, test_scenario::ctx(scenario));

    //     // Advance the epoch by 1.
    //     test_scenario::next_epoch(scenario);
    //     assert!(tx_context::epoch(test_scenario::ctx(scenario)) == 1, 1);

    //     test_scenario::next_tx(scenario, &TEST_RECIPIENT_ADDR);
    //     let locked_coin = test_scenario::take_owned<LockedCoin<SUI>>(scenario);
    //     // The unlock should fail.
    //     locked_coin::unlock_coin(locked_coin, test_scenario::ctx(scenario));
    // }
}