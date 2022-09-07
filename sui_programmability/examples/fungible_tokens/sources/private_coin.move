// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// WARNING: Like all files in the examples section, this code is unaudited
/// and should NOT be running in production. Using the code unaudited could potentially
/// result in lost of funds from hacks, and leakage of transaction amounts.

/// Module representing an example implementation for private coins. 
///
/// To implement any of the methods, module defining the type for the currency
/// is expected to implement the main set of methods such as `borrow()`,
/// `borrow_mut()` and `zero()`.
module fungible_tokens::private_coin {
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::object::{Self, UID};
    use fungible_tokens::private_balance::{Self, PrivateBalance, Supply};
    use sui::elliptic_curve::{Self as ec, RistrettoPoint};

    /// A private coin of type `T` worth `value`.
    /// The balance stores a RistrettoPoint that is a pedersen commitment of the coin's value.
    /// The coin may be public or private.
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

    // === Balance <-> PrivacyCoin accessors and type morphing ===

    /// Get immutable reference to the balance of a coin.
    public fun balance<T>(coin: &PrivateCoin<T>): &PrivateBalance<T> {
        &coin.balance
    }

    /// Get a mutable reference to the balance of a coin.
    public fun balance_mut<T>(coin: &mut PrivateCoin<T>): &mut PrivateBalance<T> {
        &mut coin.balance
    }

    /// Wrap a private balance into a PrivateCoin to make it transferable.
    public fun from_balance<T>(balance: PrivateBalance<T>, ctx: &mut TxContext): PrivateCoin<T> {
        PrivateCoin { id: object::new(ctx), balance }
    }

    /// Destruct a PrivateCoin wrapper and keep the balance.
    public fun into_balance<T>(coin: PrivateCoin<T>): PrivateBalance<T> {
        let PrivateCoin { id, balance } = coin;
        object::delete(id);
        balance
    }

    /// Take a `PrivateCoin` of `value` worth from `PrivateBalance`.
    /// Aborts if `balance.value > Open(new_commitment)`
    public fun take<T>(
        balance: &mut PrivateBalance<T>, new_commitment: RistrettoPoint, proof: vector<u8>, ctx: &mut TxContext,
    ): PrivateCoin<T> {
        PrivateCoin {
            id: object::new(ctx),
            balance: private_balance::split(balance, new_commitment, proof)
        }
    }

    /// Take a `PrivateCoin` of `value` worth from `PrivateBalance`.
    /// Aborts if `value > balance.value`
    public fun take_public<T>(
        balance: &mut PrivateBalance<T>, value: u64, proof: vector<u8>, ctx: &mut TxContext,
    ): PrivateCoin<T> {
        PrivateCoin {
            id: object::new(ctx),
            balance: private_balance::split_to_public(balance, value, proof)
        }
    }

    /// Put a `PrivateCoin<T>` into a `PrivateBalance<T>`.
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
    public entry fun join<T>(self: &mut PrivateCoin<T>, c: PrivateCoin<T>) {
        let PrivateCoin { id, balance } = c;
        object::delete(id);
        private_balance::join(&mut self.balance, balance);
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
        cap: &mut TreasuryCap<T>, value: u64, ctx: &mut TxContext,
    ): PrivateCoin<T> {
        PrivateCoin {
            id: object::new(ctx),
            balance: private_balance::increase_supply(&mut cap.total_supply, value)
        }
    }

    /// Mint some amount of T as a `PrivateBalance` and increase the total
    /// supply in `cap` accordingly.
    /// Aborts if `value` + `cap.total_supply` >= U64_MAX
    public fun mint_balance<T>(
        cap: &mut TreasuryCap<T>, value: u64
    ): PrivateBalance<T> {
        private_balance::increase_supply(&mut cap.total_supply, value)
    }

    /// Give away the treasury cap to `recipient`
    public fun transfer_cap<T>(c: TreasuryCap<T>, recipient: address) {
        transfer::transfer(c, recipient)
    }

    // === Entrypoints ===

    /// Mint `amount` of `PrivateCoin` and send it to `recipient`. Invokes `mint()`.
    public entry fun mint_and_transfer<T>(
        c: &mut TreasuryCap<T>, amount: u64, recipient: address, ctx: &mut TxContext
    ) {
        transfer::transfer(mint(c, amount, ctx), recipient)
    }

    /// Split coin from `self`, the splitted coin will be a private coin worth the value committed by `new_commitment`,
    /// the remaining balance is left in `self`. Note that performing split on a public coin turns it into a private coin.
    public entry fun split_and_transfer<T>(
        c: &mut PrivateCoin<T>, new_commitment: vector<u8>, proof: vector<u8>, recipient: address, ctx: &mut TxContext
    ) {
        let ristretto_point = ec::new_from_bytes(new_commitment);
        transfer::transfer(take(&mut c.balance, ristretto_point, proof, ctx), recipient)
    }

    /// Split coin from `self`, the splitted coin will be a public coin worth `value`.
    /// the remaining balance is left in `self`. `self` should retain it's privacy option after this call.
    public entry fun split_public_and_transfer<T>(self: &mut PrivateCoin<T>, value: u64, proof: vector<u8>, recipient: address, ctx: &mut TxContext) {
        transfer::transfer(
            take_public(&mut self.balance, value, proof, ctx),
            recipient
        )
    }

    /// Reveals a `PrivateCoin` - allowing others to freely query the coin's balance.
    public entry fun open_coin<T>(c: &mut PrivateCoin<T>, value: u64, blinding_factor: vector<u8>) {
        private_balance::open_balance(&mut c.balance, value, blinding_factor)
    }
}

///////////////////////////////////////////
// Tests
///////////////////////////////////////////

#[test_only]
module fungible_tokens::private_coin_tests {
    use sui::test_scenario::{Self as test, Scenario, next_tx, ctx};
    use fungible_tokens::private_coin::{Self as pc};
    use fungible_tokens::private_balance::{Self as pb};
    use sui::transfer;
    use sui::elliptic_curve::{Self as ec};
    use std::option;

    /// Gonna be our test token.
    struct MIZU has drop {}

    // Tests section
    #[test] fun test_init_pool() { test_transfer_private_coin(&mut scenario()) }

    fun test_transfer_private_coin(scenario: &mut Scenario) {
        let (owner, recipient) = people();

        // Create currency
        next_tx(scenario, &owner); {
            let cap = pc::create_currency(MIZU {}, ctx(scenario));
            transfer::transfer(cap, owner);
        };

        // Mint private coin
        next_tx(scenario, &owner); {
            let cap = test::take_owned<pc::TreasuryCap<MIZU>>(scenario);
            pc::mint_and_transfer(
                &mut cap,
                1000,
                recipient,
                ctx(scenario)
            );
            test::return_owned(scenario, cap);
        };

        // Split and transfer private coin
        next_tx(scenario, &recipient); {
            let coin = test::take_owned<pc::PrivateCoin<MIZU>>(scenario);
            // This should be created off-chain in the actual implementation
            let commit = ec::create_pedersen_commitment(
                ec::new_scalar_from_u64(990u64),
                ec::new_scalar_from_u64(10u64),
            );

            // Prove that the commitment of value = 10 = 1000 - 990, with binding factor = (0 - 10) mod P
            // is within range [0, 2^64)
            let proof = vector[
                14, 20, 89, 254, 230, 98, 15, 133, 48, 167, 77, 20, 17, 248, 192, 77, 189, 172, 239, 81, 39, 191, 144, 226, 234, 171, 100, 43, 177, 204, 124, 26, 202, 67, 212, 66, 165, 41, 168, 235, 232, 142, 150, 128, 6, 135, 87, 183, 181, 252, 128, 30, 93, 188, 182, 164, 109, 254, 60, 28, 197, 71, 60, 6, 178, 128, 192, 164, 178, 105, 135, 68, 176, 251, 160, 137, 221, 206, 90, 35, 100, 122, 28, 178, 212, 24, 30, 15, 152, 222, 106, 211, 172, 1, 5, 9, 152, 106, 190, 90, 128, 47, 21, 253, 16, 176, 97, 63, 229, 179, 64, 32, 245, 61, 13, 89, 95, 118, 30, 241, 77, 157, 138, 121, 168, 9, 131, 122, 182, 88, 80, 193, 44, 183, 92, 119, 25, 241, 212, 35, 163, 83, 65, 177, 155, 152, 173, 209, 252, 43, 191, 247, 207, 36, 142, 220, 212, 56, 25, 3, 7, 2, 167, 10, 220, 15, 30, 22, 142, 51, 168, 182, 200, 97, 224, 117, 129, 177, 65, 71, 70, 78, 210, 0, 251, 22, 137, 211, 160, 54, 4, 0, 11, 187, 4, 116, 195, 103, 7, 94, 152, 195, 255, 151, 230, 6, 226, 109, 7, 189, 83, 236, 142, 111, 212, 222, 167, 126, 151, 62, 178, 156, 65, 6, 94, 142, 111, 135, 12, 127, 185, 188, 165, 161, 168, 132, 87, 109, 200, 10, 153, 224, 1, 215, 164, 129, 139, 198, 111, 82, 30, 148, 93, 61, 143, 7, 6, 33, 60, 64, 199, 150, 132, 175, 229, 115, 48, 172, 155, 153, 69, 229, 96, 193, 51, 99, 119, 230, 68, 232, 190, 185, 228, 65, 55, 184, 59, 20, 88, 210, 196, 175, 127, 67, 139, 139, 105, 191, 126, 146, 151, 81, 163, 158, 154, 227, 130, 194, 74, 166, 175, 105, 228, 0, 19, 208, 121, 97, 237, 16, 126, 130, 214, 253, 138, 125, 31, 222, 111, 246, 151, 205, 155, 35, 237, 27, 36, 83, 237, 136, 201, 63, 179, 238, 67, 240, 60, 255, 76, 81, 196, 25, 222, 212, 66, 213, 40, 145, 38, 152, 101, 243, 135, 100, 143, 193, 52, 73, 147, 26, 230, 72, 125, 66, 141, 109, 172, 111, 15, 60, 218, 27, 207, 101, 240, 10, 14, 39, 195, 194, 7, 204, 248, 16, 63, 88, 137, 114, 76, 184, 67, 116, 246, 100, 68, 255, 98, 96, 121, 181, 172, 134, 90, 93, 59, 62, 154, 6, 3, 87, 185, 247, 169, 54, 246, 80, 38, 103, 84, 90, 136, 193, 76, 5, 155, 174, 93, 153, 58, 248, 66, 34, 218, 82, 53, 222, 97, 83, 242, 252, 65, 49, 51, 33, 197, 195, 40, 218, 231, 252, 190, 30, 162, 7, 155, 138, 189, 229, 240, 175, 49, 1, 11, 118, 195, 52, 167, 179, 56, 82, 232, 119, 16, 74, 178, 236, 206, 66, 90, 225, 229, 184, 121, 56, 254, 191, 7, 10, 241, 58, 162, 252, 95, 171, 5, 137, 201, 153, 22, 203, 9, 69, 236, 60, 234, 25, 6, 151, 244, 130, 110, 173, 70, 157, 162, 247, 193, 127, 210, 218, 119, 3, 99, 26, 215, 37, 88, 250, 41, 19, 62, 112, 190, 7, 134, 188, 46, 25, 230, 128, 83, 65, 183, 38, 54, 180, 25, 188, 128, 76, 128, 106, 119, 225, 232, 164, 33, 90, 16, 1, 196, 193, 213, 211, 48, 70, 240, 124, 103, 104, 99, 196, 153, 226, 231, 190, 173, 28, 211, 2, 177, 218, 86, 149, 225, 255, 254, 218, 170, 206, 91, 79, 33, 32, 81, 205, 232, 122, 192, 92, 146, 186, 164, 21, 129, 203, 148, 219, 145, 98, 125, 230, 164, 35, 252, 84, 210, 113, 201, 233, 23, 56, 104, 253, 209, 6, 225, 7, 136, 1, 13, 96, 137, 122, 44, 173, 45, 78, 53, 68, 102, 172, 61, 155, 130, 30, 124, 173, 2, 15, 0, 150, 98, 111, 3, 249, 216, 151, 142, 7, 251, 13
            ];

            pc::split_and_transfer<MIZU>(
                &mut coin,
                ec::bytes(&commit),
                proof,
                owner,
                ctx(scenario)
            );

            test::return_owned(scenario, coin);
        };

        // Check that balances are correct when opened
        next_tx(scenario, &recipient); {
            let coin = test::take_owned<pc::PrivateCoin<MIZU>>(scenario);
            let value = 10u64;
            // (0 - 10) mod P
            let blinding = vector[227, 211, 245, 92, 26, 99, 18, 88, 214, 156, 247, 162, 222, 249, 222, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 16];
            // Open the coin
            pc::open_coin(&mut coin, value, blinding);
            // Get the opened public balance and check that this is accurate
            assert!(*option::borrow(&pb::value(pc::balance(&coin))) == value, 0);
            test::return_owned(scenario, coin);
        };

        // Transfer a public coin out. Ensure that this still remains public
        next_tx(scenario, &recipient); {
            let coin = test::take_owned<pc::PrivateCoin<MIZU>>(scenario);
            let value = 1u64;
            // Open the coin
            pc::split_public_and_transfer<MIZU>(
                &mut coin,
                value,
                vector[],
                owner,
                ctx(scenario)
            );
            // Check that the coin is still public and equals to 9
            assert!(*option::borrow(&pb::value(pc::balance(&coin))) == 9, 0);
            test::return_owned(scenario, coin);
        };
    }
    // utilities
    fun scenario(): Scenario { test::begin(&@0x1) }
    fun people(): (address, address) { (@0xB0BA, @0x7EA) }
}
