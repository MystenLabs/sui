#[evm_contract]
/// An implementation of ERC20.
module Evm::ERC20Token {
    use Evm::Evm::{sender, self, sign};
    use std::errors;
    use std::vector;

    #[storage]
    /// Represents the state of this contract. This is located at `borrow_global<State>(self())`.
    struct State has key {
        decimals: u8,
        total_supply: u128,
    }

    #[storage]
    /// Represents the state of an account managed by this contract, located at
    /// `borrow_global<Account>(address)`. Notice that the storage of this resource
    /// is private to this contract, as each EVM contract manages its own Move address space.
    struct Account has key {
        /// The balance value.
        value: u128,
        /// The allowances this account has granted to other specified accounts.
        allowances: vector<Allowance>
    }

    /// How much a spender is allowed to use.
    struct Allowance has store {
        spender: address,
        amount: u128,
    }

    #[create]
    /// Constructor of this contract.
    public fun create(initial_amount: u128, decimals: u8) {
        // Initial state of contract
        move_to<State>(&sign(self()), State{decimals, total_supply: initial_amount});

        // Initialize senders balance with initial amount
        move_to<Account>(&sign(sender()), Account{value: initial_amount, allowances: vector::empty()});
    }

    #[callable, view]
    /// Returns the total supply of the token.
    public fun total_supply(): u128 acquires State {
        borrow_global<State>(self()).total_supply
    }

    #[callable, view]
    /// Returns the balance of an account.
    public fun balance_of(owner: address): u128 acquires Account {
        borrow_global<Account>(owner).value
    }

    #[callable, view]
    /// Returns the allowance an account owner has approved for the given spender.
    public fun allowance(owner: address, spender: address): u128 acquires Account {
        let allowances = &borrow_global<Account>(owner).allowances;
        let i = index_of_allowance(allowances, spender);
        if (i < vector::length(allowances)) {
            vector::borrow(allowances, i).amount
        } else {
            0
        }
    }

    #[callable]
    /// Approves that the spender can spent the given amount on behalf of the calling account.
    public fun approve(spender: address, amount: u128) acquires Account {
        create_account_if_not_present(sender());
        let allowances = &mut borrow_global_mut<Account>(sender()).allowances;
        mut_allowance(allowances, spender).amount = amount
    }

    #[callable]
    /// Transfers the amount from the sending account to the given account
    public fun transfer(to: address, amount: u128) acquires Account {
        assert!(sender() != to, errors::invalid_argument(0));
        do_transfer(sender(), to, amount)
    }


    #[callable]
    /// Transfers the amount on behalf of the `from` account to the given account.
    /// This evaluates and adjusts the allowance.
    public fun transfer_from(from: address, to: address, amount: u128) acquires Account {
        let allowances = &mut borrow_global_mut<Account>(from).allowances;
        let allowance = mut_allowance(allowances, sender());
        assert!(allowance.amount >= amount, errors::limit_exceeded(0));
        allowance.amount = allowance.amount - amount;
        do_transfer(from, to, amount)
    }

    /// Helper function to perform a transfer of funds.
    fun do_transfer(from: address, to: address, amount: u128) acquires Account {
        create_account_if_not_present(from);
        create_account_if_not_present(to);
        let from_acc = borrow_global_mut<Account>(from);
        assert!(from_acc.value >= amount, errors::limit_exceeded(0));
        from_acc.value = from_acc.value - amount;
        let to_acc = borrow_global_mut<Account>(to);
        to_acc.value = to_acc.value + amount;
    }

    /// Helper function to find the index of an existing allowance. Returns length of the passed
    /// vector if not present.
    fun index_of_allowance(allowances: &vector<Allowance>, spender: address): u64 {
        let i = 0;
        let l = vector::length(allowances);
        while (i < l) {
            if (vector::borrow(allowances, i).spender == spender) {
                return i
            };
            i = i + 1;
       };
       return l
    }

    /// Helper function to return a mut ref to the allowance of a spender.
    fun mut_allowance(allowances: &mut vector<Allowance>, spender: address): &mut Allowance {
        let i = index_of_allowance(allowances, spender);
        if (i == vector::length(allowances)) {
            vector::push_back(allowances, Allowance{spender, amount: 0})
        };
        vector::borrow_mut(allowances, i)
    }

    /// Helper function to create an account with a zero balance and no allowances.
    fun create_account_if_not_present(owner: address) {
        if (!exists<Account>(owner)) {
            move_to<Account>(&sign(owner), Account{value: 0, allowances: vector::empty()})
        }
    }

    // ==============================================================================================================
    // The following APIs will be automatically generated from the #[callable] function attributes. They
    // constitute the EVM level contract API.

    public native fun call_total_supply(contract: address): u128;
    public native fun call_allowance(contract: address, owner: address, spender: address): u128;
    public native fun call_approve(contract: address, spender: address, amount: u128);
    public native fun call_transfer(contract: address, to: address, amount: u128);
    public native fun call_transfer_from(contract: address, from: address, to: address, amount: u128);

    // ... and the same as delegate_XXXX APIs?
}
