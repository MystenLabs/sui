#[evm_contract]
/// An implementation of the ERC-20 Token Standard.
module Evm::ERC20Mock {
    use Evm::Evm::{sender, emit, require};
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

    #[storage]
    /// Represents the state of this contract.
    struct State has key {
        balances: Table<address, U256>,
        allowances: Table<address, Table<address, U256>>,
        total_supply: U256,
        name: vector<u8>,
        symbol: vector<u8>,
    }

    // ----------------------------------------
    // For test only
    // ----------------------------------------
    #[callable(sig=b"mint(address,uint256)")]
    public fun mint(state: &mut State, account: address, amount: U256) {
        mint_(state, account, amount)
    }

    #[callable(sig=b"burn(address,uint256)")]
    public fun burn(state: &mut State, account: address, amount: U256) {
        burn_(state, account, amount)
    }

    #[callable(sig=b"transferInternal(address,address,uint256)")]
    public fun transferInternal(state: &mut State, from: address, to: address, value: U256) {
        transfer_(state, from, to, value)
    }

    #[callable(sig=b"approveInternal(address,address,uint256)")]
    public fun approveInternal(state: &mut State, owner: address, spender: address, value: U256) {
        approve_(state, owner, spender, value)
    }
    // ----------------------------------------
    // End of "For test only"
    // ----------------------------------------

    #[create(sig=b"constructor(string,string,address,uint256)")]
    /// Constructor of this contract.
    public fun create(name: vector<u8>, symbol: vector<u8>, initial_account: address, initial_balance: U256): State {
        // Initial state of contract
        let state = State {
            total_supply: U256::zero(),
            balances: Table::empty<address, U256>(),
            allowances: Table::empty<address, Table<address, U256>>(),
            name,
            symbol,
        };
        // Minting the initial supply
        mint_(&mut state, initial_account, initial_balance);
        state
    }

    #[callable(sig=b"name() returns (string)"), view]
    /// Returns the name of the token
    public fun name(state: &State): vector<u8> {
        state.name
    }

    #[callable(sig=b"symbol() returns (string)"), view]
    /// Returns the symbol of the token, usually a shorter version of the name.
    public fun symbol(state: &State): vector<u8> {
        state.symbol
    }

    #[callable(sig=b"decimals() returns (uint8)"), view]
    /// Returns the number of decimals used to get its user representation.
    public fun decimals(): u8 {
        18
    }

    #[callable(sig=b"totalSupply() returns (uint256)"), view]
    /// Returns the total supply of the token.
    public fun totalSupply(state: &State): U256 {
        state.total_supply
    }

    #[callable(sig=b"balanceOf(address) returns (uint256)"), view]
    /// Returns the balance of an account.
    public fun balanceOf(state: &mut State, owner: address): U256 {
        *mut_balanceOf(state, owner)
    }

    #[callable(sig=b"transfer(address,uint256) returns (bool)")]
    /// Transfers the amount from the sending account to the given account
    public fun transfer(state: &mut State, to: address, amount: U256): bool {
        transfer_(state, sender(), to, amount);
        true
    }

    #[callable(sig=b"transferFrom(address,address,uint256) returns (bool)")]
    /// Transfers the amount on behalf of the `from` account to the given account.
    /// This evaluates and adjusts the allowance.
    public fun transferFrom(state: &mut State, from: address, to: address, amount: U256): bool {
        spendAllowance_(state, from, sender(), amount);
        transfer_(state, from, to, amount);
        true
    }

    #[callable(sig=b"approve(address,uint256) returns (bool)")]
    /// Approves that the spender can spent the given amount on behalf of the calling account.
    public fun approve(state: &mut State, spender: address, amount: U256): bool {
        approve_(state, sender(), spender, amount);
        true
    }

    #[callable(sig=b"allowance(address,address) returns (uint256)"), view]
    /// Returns the allowance an account owner has approved for the given spender.
    public fun allowance(state: &mut State, owner: address, spender: address): U256 {
        *mut_allowance(state, owner, spender)
    }

    #[callable(sig=b"increaseAllowance(address,uint256) returns (bool)")]
    /// Atomically increases the allowance granted to `spender` by the caller.
    public fun increaseAllowance(state: &mut State, spender: address, addedValue: U256): bool {
        let owner = sender();
        let increased = U256::add(allowance(state, owner, spender), addedValue);
        approve_(state, owner, spender, increased);
        true
    }

    #[callable(sig=b"decreaseAllowance(address,uint256) returns (bool)")]
    /// Atomically decreases the allowance granted to `spender` by the caller.
    public fun decreaseAllowance(state: &mut State, spender: address, subtractedValue: U256): bool {
        let owner = sender();
        let currentAllowance = allowance(state, owner, spender);
        require(U256::ge(currentAllowance, subtractedValue), b"ERC20: decreased allowance below zero");
        approve_(state, owner, spender, U256::sub(currentAllowance, subtractedValue));
        true
    }

    /// Helper function to update `owner` s allowance for `spender` based on spent `amount`.
    fun spendAllowance_(state: &mut State, owner: address, spender: address, amount: U256) {
        let currentAllowance = allowance(state, owner, spender);
        if (currentAllowance != U256::max()) {
            require(U256::ge(currentAllowance, amount), b"ERC20: insufficient allowance");
            approve_(state, owner, spender, U256::sub(currentAllowance, amount));
        }
    }

    /// Helper function to perform an approval
    fun approve_(state: &mut State, owner: address, spender: address, amount: U256) {
        require(owner != @0x0, b"ERC20: approve from the zero address");
        require(spender != @0x0, b"ERC20: approve to the zero address");
        if(!Table::contains(&state.allowances, &owner)) {
            Table::insert(&mut state.allowances, &owner, Table::empty<address, U256>())
        };
        let a = Table::borrow_mut(&mut state.allowances, &owner);
        if(!Table::contains(a, &spender)) {
            Table::insert(a, &spender, amount);
        }
        else {
            *Table::borrow_mut(a, &spender) = amount;
        };
        emit(Approval{owner, spender, value: amount});
    }

    /// Helper function to perform a transfer of funds.
    fun transfer_(state: &mut State, from: address, to: address, amount: U256) {
        require(from != @0x0, b"ERC20: transfer from the zero address");
        require(to != @0x0, b"ERC20: transfer to the zero address");
        let from_bal = mut_balanceOf(state, from);
        require(U256::le(amount, *from_bal), b"ERC20: transfer amount exceeds balance");
        *from_bal = U256::sub(*from_bal, amount);
        let to_bal = mut_balanceOf(state, to);
        *to_bal = U256::add(*to_bal, copy amount);
        emit(Transfer{from, to, value: amount});
    }

    /// Helper function to return a mut ref to the allowance of a spender.
    fun mut_allowance(state: &mut State, owner: address, spender: address): &mut U256 {
        if(!Table::contains(&state.allowances, &owner)) {
            Table::insert(&mut state.allowances, &owner, Table::empty<address, U256>())
        };
        let allowance_owner = Table::borrow_mut(&mut state.allowances, &owner);
        Table::borrow_mut_with_default(allowance_owner, &spender, U256::zero())
    }

    /// Helper function to return a mut ref to the balance of a owner.
    fun mut_balanceOf(state: &mut State, owner: address): &mut U256 {
        Table::borrow_mut_with_default(&mut state.balances, &owner, U256::zero())
    }

    /// Create `amount` tokens and assigns them to `account`, increasing
    /// the total supply.
    fun mint_(state: &mut State, account: address, amount: U256) {
        require(account != @0x0, b"ERC20: mint to the zero address");
        state.total_supply = U256::add(state.total_supply, amount);
        let mut_bal_account = mut_balanceOf(state, account);
        *mut_bal_account = U256::add(*mut_bal_account, amount);
        emit(Transfer{from: @0x0, to: account, value: amount});
    }

    /// Destroys `amount` tokens from `account`, reducing the total supply.
    fun burn_(state: &mut State, account: address, amount: U256) {
        require(account != @0x0, b"ERC20: burn from the zero address");
        let mut_bal_account = mut_balanceOf(state, account);
        require(U256::ge(*mut_bal_account, amount), b"ERC20: burn amount exceeds balance");
        *mut_bal_account = U256::sub(*mut_bal_account, amount);
        state.total_supply = U256::sub(state.total_supply, amount);
        emit(Transfer{from: account, to: @0x0, value: amount});
    }
}
