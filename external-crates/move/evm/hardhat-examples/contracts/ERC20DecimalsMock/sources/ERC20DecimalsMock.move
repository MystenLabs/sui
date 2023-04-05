#[evm_contract]
/// An implementation of the ERC-20 Token Standard.
module Evm::ERC20DecimalsMock {
    use Evm::Evm::{sender, self, sign, emit, require};
    use Evm::Table::{Self, Table};
    use Evm::U256::{Self, U256};

    #[event(sig=b"Transfer(address indexed,address indexed,uint256)")]
    struct Transfer {
        from: address,
        to: address,
        value: U256,
    }

    #[event(sig=b"Approval(address indexed,address indexed,uint256)")]
    struct Approval {
        owner: address,
        spender: address,
        value: U256,
    }

    /// Represents the state of this contract. This is located at `borrow_global<State>(self())`.
    struct State has key {
        balances: Table<address, U256>,
        allowances: Table<address, Table<address, U256>>,
        total_supply: U256,
        name: vector<u8>,
        symbol: vector<u8>,
        decimals: u8,
    }

    #[create(sig=b"constructor(string,string,uint8)")]
    /// Constructor of this contract.
    public fun create(name: vector<u8>, symbol: vector<u8>, decimals: u8) {
        // Initial state of contract
        move_to<State>(
            &sign(self()),
            State {
                total_supply: U256::zero(),
                balances: Table::empty<address, U256>(),
                allowances: Table::empty<address, Table<address, U256>>(),
                name,
                symbol,
                decimals,
            }
        );
    }

    #[callable(sig=b"name() returns (string)"), view]
    /// Returns the name of the token
    public fun name(): vector<u8> acquires State {
        *&borrow_global<State>(self()).name
    }

    #[callable(sig=b"symbol() returns (string)"), view]
    /// Returns the symbol of the token, usually a shorter version of the name.
    public fun symbol(): vector<u8> acquires State {
        *&borrow_global<State>(self()).symbol
    }

    #[callable(sig=b"decimals() returns (uint8)"), view]
    /// Returns the number of decimals used to get its user representation.
    public fun decimals(): u8 acquires State {
        *&borrow_global<State>(self()).decimals
    }

    #[callable(sig=b"totalSupply() returns (uint256)"), view]
    /// Returns the total supply of the token.
    public fun totalSupply(): U256 acquires State {
        *&borrow_global<State>(self()).total_supply
    }

    #[callable(sig=b"balanceOf(address) returns (uint256)"), view]
    /// Returns the balance of an account.
    public fun balanceOf(owner: address): U256 acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_balanceOf(s, owner)
    }

    #[callable(sig=b"transfer(address,uint256) returns (bool)")]
    /// Transfers the amount from the sending account to the given account
    public fun transfer(to: address, amount: U256): bool acquires State {
        transfer_(sender(), to, amount);
        true
    }

    #[callable(sig=b"transferFrom(address,address,uint256) returns (bool)")]
    /// Transfers the amount on behalf of the `from` account to the given account.
    /// This evaluates and adjusts the allowance.
    public fun transferFrom(from: address, to: address, amount: U256): bool acquires State {
        spendAllowance_(from, sender(), amount);
        transfer_(from, to, amount);
        true
    }

    #[callable(sig=b"approve(address,uint256) returns (bool)")]
    /// Approves that the spender can spent the given amount on behalf of the calling account.
    public fun approve(spender: address, amount: U256): bool acquires State {
        approve_(sender(), spender, amount);
        true
    }

    #[callable(sig=b"allowance(address,address) returns (uint256)"), view]
    /// Returns the allowance an account owner has approved for the given spender.
    public fun allowance(owner: address, spender: address): U256 acquires State {
        let s = borrow_global_mut<State>(self());
        *mut_allowance(s, owner, spender)
    }

    #[callable(sig=b"increaseAllowance(address,uint256) returns (bool)")]
    /// Atomically increases the allowance granted to `spender` by the caller.
    public fun increaseAllowance(spender: address, addedValue: U256): bool acquires State {
        let owner = sender();
        approve_(owner, spender, U256::add(allowance(owner, spender), addedValue));
        true
    }

    #[callable(sig=b"decreaseAllowance(address,uint256) returns (bool)")]
    /// Atomically decreases the allowance granted to `spender` by the caller.
    public fun decreaseAllowance(spender: address, subtractedValue: U256): bool acquires State {
        let owner = sender();
        let currentAllowance = allowance(owner, spender);
        require(U256::ge(currentAllowance, subtractedValue), b"ERC20: decreased allowance below zero");
        approve_(owner, spender, U256::sub(currentAllowance, subtractedValue));
        true
    }

    /// Helper function to update `owner` s allowance for `spender` based on spent `amount`.
    fun spendAllowance_(owner: address, spender: address, amount: U256) acquires State {
        let currentAllowance = allowance(owner, spender);
        if (currentAllowance != U256::max()) {
            require(U256::ge(currentAllowance, amount), b"ERC20: insufficient allowance");
            approve_(owner, spender, U256::sub(currentAllowance, amount));
        }
    }

    /// Helper function to perform an approval
    fun approve_(owner: address, spender: address, amount: U256) acquires State {
        require(owner != @0x0, b"ERC20: approve from the zero address");
        require(spender != @0x0, b"ERC20: approve to the zero address");
        let s = borrow_global_mut<State>(self());
        if(!Table::contains(&s.allowances, &owner)) {
            Table::insert(&mut s.allowances, &owner, Table::empty<address, U256>())
        };
        let a = Table::borrow_mut(&mut s.allowances, &owner);
        if(!Table::contains(a, &spender)) {
            Table::insert(a, &spender, amount);
        }
        else {
            *Table::borrow_mut(a, &spender) = amount;
        };
        emit(Approval{owner, spender, value: amount});
    }

    /// Helper function to perform a transfer of funds.
    fun transfer_(from: address, to: address, amount: U256) acquires State {
        require(from != @0x0, b"ERC20: transfer from the zero address");
        require(to != @0x0, b"ERC20: transfer to the zero address");
        let s = borrow_global_mut<State>(self());
        let from_bal = mut_balanceOf(s, from);
        require(U256::le(copy amount, *from_bal), b"ERC20: transfer amount exceeds balance");
        *from_bal = U256::sub(*from_bal, copy amount);
        let to_bal = mut_balanceOf(s, to);
        *to_bal = U256::add(*to_bal, copy amount);
        emit(Transfer{from, to, value: amount});
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
    fun mint_(account: address, amount: U256) acquires State {
        require(account != @0x0, b"ERC20: mint to the zero address");
        let s = borrow_global_mut<State>(self());
        s.total_supply = U256::add(s.total_supply, amount);
        let mut_bal_account = mut_balanceOf(s, account);
        *mut_bal_account = U256::add(*mut_bal_account, amount);
        emit(Transfer{from: @0x0, to: account, value: amount});
    }

    /// Destroys `amount` tokens from `account`, reducing the total supply.
    fun burn_(account: address, amount: U256) acquires State {
        require(account != @0x0, b"ERC20: burn from the zero address");
        let s = borrow_global_mut<State>(self());
        let mut_bal_account = mut_balanceOf(s, account);
        require(U256::ge(*mut_bal_account, amount), b"ERC20: burn amount exceeds balance");
        *mut_bal_account = U256::sub(*mut_bal_account, amount);
        s.total_supply = U256::sub(s.total_supply, amount);
        emit(Transfer{from: account, to: @0x0, value: amount});
    }
}
