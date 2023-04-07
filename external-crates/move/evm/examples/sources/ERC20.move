#[evm_contract]
/// An implementation of the ERC-20 Token Standard.
module Evm::ERC20 {
    use Evm::Evm::{sender, self, sign, emit};
    use Evm::Table::{Self, Table};
    use Evm::U256::{Self, U256};
    use std::ascii::{String};
    use std::errors;

    #[event]
    struct Transfer {
        from: address,
        to: address,
        value: U256,
    }

    #[event]
    struct Approval {
        owner: address,
        spender: address,
        value: U256,
    }

    #[storage]
    /// Represents the state of this contract. This is located at `borrow_global<State>(self())`.
    struct State has key {
        balances: Table<address, U256>,
        allowances: Table<address, Table<address, U256>>,
        total_supply: U256,
        name: String,
        symbol: String,
    }

    #[create]
    /// Constructor of this contract.
    public fun create(name: String, symbol: String, initial_supply: U256) acquires State {
        // Initial state of contract
        move_to<State>(
            &sign(self()),
            State {
                total_supply: U256::zero(),
                balances: Table::empty<address, U256>(),
                allowances: Table::empty<address, Table<address, U256>>(),
                name,
                symbol,
            }
        );
        // Minting the initial supply
        mint(sender(), initial_supply);
    }

    #[callable, view]
    /// Returns the name of the token
    public fun name(): String acquires State {
        *&borrow_global<State>(self()).name
    }

    #[callable, view]
    /// Returns the symbol of the token, usually a shorter version of the name.
    public fun symbol(): String acquires State {
        *&borrow_global<State>(self()).symbol
    }

    #[callable, view]
    /// Returns the number of decimals used to get its user representation.
    public fun decimals(): u8 {
        18
    }

    #[callable, view]
    /// Returns the total supply of the token.
    public fun totalSupply(): U256 acquires State {
        *&borrow_global<State>(self()).total_supply
    }

    #[callable, view]
    /// Returns the balance of an account.
    public fun balanceOf(owner: address): U256 acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_balanceOf(s, owner)
    }

    #[callable]
    /// Transfers the amount from the sending account to the given account
    public fun transfer(to: address, amount: U256): bool acquires State {
        assert!(sender() != to, errors::invalid_argument(0));
        do_transfer(sender(), to, amount);
        true
    }

    #[callable]
    /// Transfers the amount on behalf of the `from` account to the given account.
    /// This evaluates and adjusts the allowance.
    public fun transferFrom(from: address, to: address, amount: U256): bool acquires State {
        assert!(sender() != to, errors::invalid_argument(0));
        let s = borrow_global_mut<State>(self());
        let allowance_for_sender = mut_allowance(s, from, sender());
        assert!(U256::le(copy amount, *allowance_for_sender), errors::limit_exceeded(0));
        *allowance_for_sender = U256::sub(*allowance_for_sender, copy amount);
        do_transfer(from, to, amount);
        true
    }

    #[callable]
    /// Approves that the spender can spent the given amount on behalf of the calling account.
    public fun approve(spender: address, amount: U256): bool acquires State {
        let s = borrow_global_mut<State>(self());
        if(!Table::contains(&s.allowances, &sender())) {
            Table::insert(&mut s.allowances, &sender(), Table::empty<address, U256>())
        };
        let a = Table::borrow_mut(&mut s.allowances, &sender());
        Table::insert(a, &spender, copy amount);
        emit(Approval{owner: sender(), spender, value: amount});
        true
    }

    #[callable, view]
    /// Returns the allowance an account owner has approved for the given spender.
    public fun allowance(owner: address, spender: address): U256 acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_allowance(s, owner, spender)
    }

    /// Helper function to perform a transfer of funds.
    fun do_transfer(from: address, to: address, amount: U256) acquires State {
        let s = borrow_global_mut<State>(self());
        let from_bal = mut_balanceOf(s, from);
        assert!(U256::le(copy amount, *from_bal), errors::limit_exceeded(0));
        *from_bal = U256::sub(*from_bal, copy amount);
        let to_bal = mut_balanceOf(s, to);
        *to_bal = U256::add(*to_bal, copy amount);
        // TODO: Unit testing does not support events yet.
        //emit(Transfer{from, to, value: amount});
    }

    /// Helper function to return a mut ref to the allowance of a spender.
    fun mut_allowance(s: &mut State, owner: address, spender: address): &mut U256 {
        if(!Table::contains(&s.allowances, &owner)) {
            Table::insert(&mut s.allowances, &owner, Table::empty<address, U256>())
        };
        let allowance_owner = Table::borrow_mut(&mut s.allowances, &owner);
        Table::borrow_mut_with_default(allowance_owner, &spender, U256::zero())
    }

    /// Helper function to return a mut ref to the balance of a owner.
    fun mut_balanceOf(s: &mut State, owner: address): &mut U256 {
        Table::borrow_mut_with_default(&mut s.balances, &owner, U256::zero())
    }

    /// Create `amount` tokens and assigns them to `account`, increasing
    /// the total supply.
    fun mint(account: address, amount: U256) acquires State {
        let s = borrow_global_mut<State>(self());
        s.total_supply = U256::add(s.total_supply, amount);
        let mut_bal_account = mut_balanceOf(s, account);
        *mut_bal_account = U256::add(*mut_bal_account, amount);
        // TODO: Unit testing does not support events yet.
        //emit(Transfer{from: @0x0, to: account, value: amount});
    }
}
