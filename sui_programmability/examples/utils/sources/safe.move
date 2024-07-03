// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The Safe standard is a minimalistic shared wrapper around a coin. It provides a way for users to provide third-party dApps with
/// the capability to transfer coins away from their wallets, if they are provided with the correct permission.
module utils::safe {
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::vec_set::{Self, VecSet};

    // Errors
    const EInvalidTransferCapability: u64 = 0;
    const EInvalidOwnerCapability: u64 = 1;
    const ETransferCapabilityRevoked: u64 = 2;
    const EOverdrawn: u64 = 3;

    //
    /// Allows any holder of a capability to transfer a fixed amount of assets from the safe.
    /// Useful in situations like an NFT marketplace where you wish to buy the NFTs at a specific price.
    ///
    /// @ownership: Shared
    ///
    public struct Safe<phantom T> has key {
        id: UID,
        balance: Balance<T>,
        allowed_safes: VecSet<ID>,
    }

    public struct OwnerCapability<phantom T> has key, store {
        id: UID,
        safe_id: ID,
    }

    ///
    /// Allows the owner of the capability to take `amount` of coins from the box.
    ///
    /// @ownership: Owned
    ///
    public struct TransferCapability<phantom T> has store, key {
        id: UID,
        safe_id: ID,
        // The amount that the user is able to transfer.
        amount: u64,
    }

    //////////////////////////////////////////////////////
    /// HELPER FUNCTIONS
    //////////////////////////////////////////////////////

    /// Check that the capability has not yet been revoked by the owner.
    fun check_capability_validity<T>(safe: &Safe<T>, capability: &TransferCapability<T>) {
        // Check that the ids match
        assert!(object::id(safe) == capability.safe_id, EInvalidTransferCapability);
        // Check that it has not been cancelled
        assert!(safe.allowed_safes.contains(&object::id(capability)), ETransferCapabilityRevoked);
    }

    fun check_owner_capability_validity<T>(safe: &Safe<T>, capability: &OwnerCapability<T>) {
        assert!(object::id(safe) == capability.safe_id, EInvalidOwnerCapability);
    }

    /// Helper function to create a capability.
    fun create_capability_<T>(safe: &mut Safe<T>, withdraw_amount: u64, ctx: &mut TxContext): TransferCapability<T> {
        let cap_id = object::new(ctx);
        safe.allowed_safes.insert(cap_id.uid_to_inner());

        let capability = TransferCapability {
            id: cap_id,
            safe_id: safe.id.uid_to_inner(),
            amount: withdraw_amount,
        };

        capability
    }

    //////////////////////////////////////////////////////
    /// PUBLIC FUNCTIONS
    //////////////////////////////////////////////////////

    public fun balance<T>(safe: &Safe<T>): &Balance<T> {
        &safe.balance
    }

    /// Wrap a coin around a safe.
    /// a trusted party (or smart contract) to transfer the object out.
    public fun create_<T>(balance: Balance<T>, ctx: &mut TxContext): OwnerCapability<T> {
        let safe = Safe {
            id: object::new(ctx),
            balance,
            allowed_safes: vec_set::empty(),
        };
        let cap = OwnerCapability {
            id: object::new(ctx),
            safe_id: object::id(&safe),
        };
        transfer::share_object(safe);
        cap
    }

    public entry fun create<T>(coin: Coin<T>, ctx: &mut TxContext) {
        let balance = coin.into_balance();
        let cap = create_<T>(balance, ctx);
        transfer::public_transfer(cap, ctx.sender());
    }

    public entry fun create_empty<T>(ctx: &mut TxContext) {
        let empty_balance = balance::zero<T>();
        let cap = create_(empty_balance, ctx);
        transfer::public_transfer(cap, ctx.sender());
    }

    /// Deposit funds to the safe
    public fun deposit_<T>(safe: &mut Safe<T>, balance: Balance<T>) {
        safe.balance.join(balance);
    }

    /// Deposit funds to the safe
    public entry fun deposit<T>(safe: &mut Safe<T>, coin: Coin<T>) {
        let balance = coin.into_balance();
        deposit_<T>(safe, balance);
    }

    /// Withdraw coins from the safe as a `OwnerCapability` holder
    public fun withdraw_<T>(safe: &mut Safe<T>, capability: &OwnerCapability<T>, withdraw_amount: u64): Balance<T> {
        // Ensures that only the owner can withdraw from the safe.
        check_owner_capability_validity(safe, capability);
        safe.balance.split(withdraw_amount)
    }

    /// Withdraw coins from the safe as a `OwnerCapability` holder
    public entry fun withdraw<T>(safe: &mut Safe<T>, capability: &OwnerCapability<T>, withdraw_amount: u64, ctx: &mut TxContext) {
        let balance = withdraw_(safe, capability, withdraw_amount);
        let coin = coin::from_balance(balance, ctx);
        transfer::public_transfer(coin, ctx.sender());
    }

    /// Withdraw coins from the safe as a `TransferCapability` holder.
    public fun debit<T>(safe: &mut Safe<T>, capability: &mut TransferCapability<T>, withdraw_amount: u64): Balance<T> {
        // Check the validity of the capability
        check_capability_validity(safe, capability);

        // Withdraw funds
        assert!(capability.amount >= withdraw_amount, EOverdrawn);
        capability.amount = capability.amount - withdraw_amount;
        safe.balance.split(withdraw_amount)
    }

    /// Revoke a `TransferCapability` as an `OwnerCapability` holder
    public entry fun revoke_transfer_capability<T>(safe: &mut Safe<T>, capability: &OwnerCapability<T>, capability_id: ID) {
        // Ensures that only the owner can withdraw from the safe.
        check_owner_capability_validity(safe, capability);
        safe.allowed_safes.remove(&capability_id);
    }

    /// Revoke a `TransferCapability` as its owner
    public entry fun self_revoke_transfer_capability<T>(safe: &mut Safe<T>, capability: &TransferCapability<T>) {
        check_capability_validity(safe, capability);
        safe.allowed_safes.remove(&object::id(capability));
    }

    /// Create `TransferCapability` as an `OwnerCapability` holder
    public fun create_transfer_capability<T>(safe: &mut Safe<T>, capability: &OwnerCapability<T>, withdraw_amount: u64, ctx: &mut TxContext): TransferCapability<T> {
        // Ensures that only the owner can withdraw from the safe.
        check_owner_capability_validity(safe, capability);
        create_capability_(safe, withdraw_amount, ctx)
    }
}
