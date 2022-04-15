// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::Validator {
    use Std::ASCII::{Self, String};

    use Sui::Coin::{Self, Coin};
    use Sui::ID::{Self, VersionedID};
    use Sui::SUI::SUI;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    friend Sui::Genesis;
    friend Sui::ValidatorSet;

    const EADDRESS_MISMATCH: u64 = 0;

    struct Validator has key, store {
        id: VersionedID,
        sui_address: address,
        name: String,
        ip_address: vector<u8>,
        stake: Coin<SUI>,
    }

    public(friend) fun new(
        sui_address: address,
        name: vector<u8>,
        ip_address: vector<u8>,
        stake: Coin<SUI>,
        ctx: &mut TxContext,
    ): Validator {
        Validator {
            id: TxContext::new_id(ctx),
            sui_address,
            name: ASCII::string(name),
            ip_address,
            stake,
        }
    }

    public(friend) fun send_back(validator: Validator) {
        let owner = validator.sui_address;
        Transfer::transfer(validator, owner)
    }

    public(script) fun create(
        init_stake: Coin<SUI>,
        name: vector<u8>,
        ip_address: vector<u8>,
        ctx: &mut TxContext,
    ) {
        let sender = TxContext::sender(ctx);
        let validator = new(
            sender,
            name,
            ip_address,
            init_stake,
            ctx,
        );
        Transfer::transfer(validator, sender);
    }

    public(script) fun destroy(
        validator: Validator,
        ctx: &mut TxContext,
    ) {
        let sender = TxContext::sender(ctx);
        assert!(validator.sui_address == sender, EADDRESS_MISMATCH);

        let Validator { id, sui_address: _, name: _, ip_address: _, stake } = validator;
        Transfer::transfer(stake, sender);
        ID::delete(id);
    }

    public fun get_sui_address(self: &Validator): address {
        self.sui_address
    }

    public fun get_stake_amount(self: &Validator): u64 {
        Coin::value(&self.stake)
    }

    public fun duplicates_with(self: &Validator, other: &Validator): bool {
         self.sui_address == other.sui_address
            || self.name == other.name
            || self.ip_address == other.ip_address
    }
}