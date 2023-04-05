#[evm_contract]
module Evm::Token {
    use Evm::Evm::{self, sender, sign};
    use Evm::Table::{Self, Table};
    use Evm::U256::{Self, U256};
    use std::errors;

    /// Represents the state of this contract. This is located at `borrow_global<State>(self())`.
    struct State has key {
        total_supply: U256,
        balances: Table<address, U256>,
        name: vector<u8>,
    }

    #[create(sig=b"constructor(string)")]
    public fun create(name: vector<u8>) acquires State {
        move_to<State>(
            &sign(self()),
            State {
                total_supply: U256::zero(),
                balances: Table::empty<address, U256>(),
                name
            }
        );
        mint(sender(), U256::u256_from_u128(42));
    }

    #[callable(sig=b"name() returns (string)"), view]
    /// Returns the name of the token
    public fun name(): vector<u8> acquires State {
        *&borrow_global<State>(self()).name
    }

    #[callable(sig=b"totalSupply() returns (uint256)"), view]
    public fun totalSupply(): U256 acquires State {
        *&borrow_global<State>(self()).total_supply
    }


    #[callable(sig=b"balanceOf(address) returns (uint256)"), view]
    public fun balanceOf(owner: address): U256 acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_balanceOf(s, owner)
    }

    #[callable(sig=b"mint(address, uint256)")]
    public fun mint(account: address, amount: U256) acquires State {
        let s = borrow_global_mut<State>(self());
        s.total_supply = U256::add(s.total_supply, amount);
        let mut_bal_account = mut_balanceOf(s, account);
        *mut_bal_account = U256::add(*mut_bal_account, amount);
    }

    #[callable(sig=b"transfer(address, uint256) returns (bool)")]
    /// Transfers the amount from the sending account to the given account
    public fun transfer(to: address, amount: U256): bool acquires State {
        assert!(sender() != to, errors::invalid_argument(0));
        do_transfer(sender(), to, amount);
        true
    }

    fun do_transfer(from: address, to: address, amount: U256) acquires State {
        let s = borrow_global_mut<State>(self());
        let from_bal = mut_balanceOf(s, from);
        assert!(U256::le(copy amount, *from_bal), errors::limit_exceeded(0));
        *from_bal = U256::sub(*from_bal, copy amount);
        let to_bal = mut_balanceOf(s, to);
        *to_bal = U256::add(*to_bal, copy amount);
    }

    /// Helper function to return a mut ref to the balance of a owner.
    fun mut_balanceOf(s: &mut State, owner: address): &mut U256 {
        Table::borrow_mut_with_default(&mut s.balances, &owner, U256::zero())
    }
}
