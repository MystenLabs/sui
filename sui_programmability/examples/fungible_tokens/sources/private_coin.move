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
    use sui::crypto::{Self, RistrettoPoint};

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

    public fun open_coin<T>(c: &PrivateCoin<T>, value: u64, blinding_factor: vector<u8>): bool {
        let commitment = crypto::create_pedersen_commitment(
            crypto::big_scalar_to_vec(crypto::big_scalar_from_u64(value)),
            blinding_factor
        );
        crypto::value(&commitment) == crypto::value(&private_balance::value(&c.balance))
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
        c: &mut PrivateCoin<T>, new_commitment: vector<u8>, proof: vector<u8>, recipient: address, ctx: &mut TxContext
    ) {
        let ristretto_point = crypto::ristretto_from_bytes(new_commitment);
        transfer::transfer(take(&mut c.balance, ristretto_point, proof, ctx), recipient)
    }

    /// Split coin `self` to two coins, one with balance `split_amount`,
    /// and the remaining balance is left is `self`.
    public entry fun split<T>(self: &mut PrivateCoin<T>, new_commitment: vector<u8>, proof: vector<u8>, ctx: &mut TxContext) {
        let ristretto_point = crypto::ristretto_from_bytes(new_commitment);
        transfer::transfer(
            take(&mut self.balance, ristretto_point, proof, ctx),
            tx_context::sender(ctx)
        )
    }

    // === Test-only code ===

    // #[test_only]
    // /// Mint coins of any type for (obviously!) testing purposes only
    // public fun mint_for_testing<T>(value: u64, ctx: &mut TxContext): PrivateCoin<T> {
    //     PrivateCoin { id: object::new(ctx), balance: private_balance::create_for_testing(value) }
    // }

    // #[test_only]
    // /// Destroy a `Coin` with any value in it for testing purposes.
    // public fun destroy_for_testing<T>(self: PrivateCoin<T>): RistrettoPoint {
    //     let PrivateCoin { id, balance } = self;
    //     object::delete(id);
    //     private_balance::destroy_for_testing(balance)
    // }
}

#[test_only]
module fungible_tokens::private_coin_tests {
    use sui::test_scenario::{Self as test, Scenario, next_tx, ctx};
    use fungible_tokens::private_coin::{Self as pc};
    use sui::crypto::{Self, big_scalar_to_vec, big_scalar_from_u64, create_pedersen_commitment};
    use sui::transfer;

    /// Gonna be our test token.
    struct PRIVATE_SUI has drop {}

    // Tests section
    #[test] fun test_init_pool() { test_transfer_private_coin(&mut scenario()) }

    /// Init a Pool with a 1_000_000 BEEP and 1_000_000_000 SUI;
    /// Set the ratio BEEP : SUI = 1 : 1000.
    /// Set LSP token amount to 1000;
    fun test_transfer_private_coin(scenario: &mut Scenario) {
        let (owner, recipient) = people();

        // Create currency
        next_tx(scenario, &owner); {
            let cap = pc::create_currency(PRIVATE_SUI {}, ctx(scenario));
            transfer::transfer(cap, owner);
        };

        // Mint private coin
        next_tx(scenario, &owner); {
            let cap = test::take_owned<pc::TreasuryCap<PRIVATE_SUI>>(scenario);
            pc::mint_and_transfer(
                &mut cap,
                1000,
                big_scalar_to_vec(big_scalar_from_u64(1000u64)),
                recipient,
                ctx(scenario)
            );
            test::return_owned(scenario, cap);
        };

        // Split and transfer private coin
        next_tx(scenario, &recipient); {
            let coin = test::take_owned<pc::PrivateCoin<PRIVATE_SUI>>(scenario);
            // This should be created off-chain in the actual implementation
            let commit = create_pedersen_commitment(
                big_scalar_to_vec(big_scalar_from_u64(10u64)),
                big_scalar_to_vec(big_scalar_from_u64(20u64)),
            );

            // Prove that the commitment of 990 = 1000 - 10, with binding factor of 980 = 1000 - 20
            // is within range [0, 2^64)
            let proof = vector[14, 119, 245, 193, 236, 62, 56, 228, 253, 247, 201, 105, 4, 15, 11, 205, 155, 74, 238, 86, 88, 98, 119, 133, 247, 40, 8, 109, 33, 228, 237, 106, 28, 171, 1, 229, 157, 144, 212, 20, 54, 34, 77, 195, 159, 91, 253, 224, 248, 0, 48, 48, 121, 252, 183, 8, 81, 64, 136, 1, 212, 203, 43, 0, 156, 87, 140, 242, 240, 174, 18, 138, 4, 178, 203, 25, 62, 61, 10, 137, 244, 248, 159, 198, 61, 96, 184, 252, 133, 246, 97, 87, 93, 83, 39, 59, 200, 239, 88, 210, 22, 27, 183, 94, 135, 213, 187, 204, 208, 80, 154, 194, 90, 53, 212, 120, 58, 36, 226, 198, 31, 58, 113, 132, 58, 64, 65, 36, 27, 206, 91, 91, 246, 180, 225, 30, 48, 107, 125, 19, 15, 222, 70, 173, 18, 18, 154, 239, 139, 25, 207, 163, 204, 182, 176, 77, 235, 172, 46, 15, 201, 146, 139, 253, 184, 141, 108, 133, 227, 20, 8, 93, 61, 18, 162, 137, 189, 194, 172, 154, 221, 112, 165, 148, 55, 243, 158, 183, 46, 106, 144, 3, 90, 88, 166, 161, 100, 176, 152, 56, 246, 96, 138, 37, 152, 19, 17, 234, 139, 124, 218, 109, 155, 157, 34, 145, 174, 212, 52, 47, 155, 18, 215, 3, 214, 203, 84, 77, 251, 30, 48, 151, 167, 30, 73, 186, 77, 45, 215, 80, 231, 151, 215, 108, 151, 59, 127, 205, 126, 114, 254, 146, 19, 229, 55, 95, 64, 255, 137, 96, 55, 204, 103, 129, 95, 60, 172, 0, 79, 218, 231, 1, 145, 24, 22, 97, 213, 47, 138, 45, 26, 152, 82, 0, 20, 231, 112, 30, 120, 149, 55, 50, 130, 83, 49, 62, 48, 31, 181, 77, 245, 144, 204, 115, 42, 119, 25, 110, 171, 82, 50, 20, 72, 11, 168, 193, 250, 240, 28, 113, 70, 12, 84, 55, 155, 254, 4, 225, 9, 68, 139, 111, 222, 109, 16, 246, 150, 116, 51, 247, 242, 252, 241, 184, 135, 228, 11, 62, 206, 196, 248, 117, 244, 126, 63, 75, 113, 7, 249, 253, 117, 221, 25, 157, 159, 81, 17, 150, 112, 251, 212, 223, 133, 190, 104, 239, 243, 116, 11, 6, 214, 131, 96, 75, 186, 223, 85, 248, 114, 241, 157, 46, 235, 174, 60, 23, 86, 192, 57, 160, 188, 62, 149, 213, 12, 49, 196, 71, 176, 170, 1, 102, 36, 119, 221, 82, 192, 228, 133, 218, 102, 79, 239, 136, 179, 101, 63, 106, 31, 91, 40, 197, 171, 104, 43, 217, 208, 116, 198, 166, 5, 73, 133, 105, 147, 28, 23, 20, 194, 150, 21, 91, 228, 222, 200, 252, 125, 142, 44, 35, 127, 43, 118, 20, 164, 74, 183, 67, 172, 124, 108, 61, 141, 28, 105, 173, 179, 168, 205, 69, 192, 55, 89, 205, 239, 67, 169, 107, 24, 4, 234, 131, 90, 255, 159, 118, 81, 66, 39, 230, 97, 214, 146, 23, 164, 111, 211, 252, 108, 148, 251, 97, 200, 135, 120, 245, 172, 33, 103, 74, 44, 78, 155, 218, 166, 177, 233, 204, 205, 183, 131, 251, 45, 124, 173, 230, 60, 14, 123, 71, 209, 13, 241, 106, 110, 157, 128, 84, 140, 234, 104, 227, 7, 159, 224, 216, 181, 205, 96, 227, 36, 113, 3, 75, 11, 166, 65, 175, 180, 22, 112, 92, 73, 58, 123, 19, 134, 236, 135, 138, 20, 74, 21, 45, 22, 207, 247, 69, 237, 10, 226, 67, 51, 59, 2, 49, 229, 5, 245, 188, 121, 188, 129, 42, 98, 161, 89, 18, 119, 164, 163, 181, 245, 42, 137, 160, 154, 57, 179, 67, 0, 189, 215, 74, 40, 95, 2, 59, 100, 4, 26, 20, 58, 156, 207, 248, 149, 167, 225, 13, 190, 33, 122, 3, 65, 170, 249, 169, 39, 159, 157, 159, 118, 176, 187, 218, 81, 246, 53, 88, 234, 246, 211, 182, 225, 217, 169, 65, 245, 86, 74, 10];

            pc::split_and_transfer<PRIVATE_SUI>(
                &mut coin,
                crypto::value(&commit),
                proof,
                owner,
                ctx(scenario)
            );

            test::return_owned(scenario, coin);
        };

        // Check that balances are correct when opened
        next_tx(scenario, &owner); {
            let coin = test::take_owned<pc::PrivateCoin<PRIVATE_SUI>>(scenario);
            let value = 10u64;
            let blinding = big_scalar_to_vec(big_scalar_from_u64(20u64));

            assert!(pc::open_coin(&coin, value, blinding), 0);
            test::return_owned(scenario, coin);
        };

        // Check that balances are correct when opened
        next_tx(scenario, &recipient); {
            let coin = test::take_owned<pc::PrivateCoin<PRIVATE_SUI>>(scenario);
            let value = 990u64;
            let blinding = big_scalar_to_vec(big_scalar_from_u64(980u64));

            assert!(pc::open_coin(&coin, value, blinding), 0);
            test::return_owned(scenario, coin);
        }
    }
    // utilities
    fun scenario(): Scenario { test::begin(&@0x1) }
    fun people(): (address, address) { (@0xB0BA, @0x7EA) }
}
