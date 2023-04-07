#[evm_contract]
/// An implementation of ERC20.
module 0x2::ERC20 {
    use Evm::U256::{U256, u256_from_words};

    #[callable(sig=b"totalSupply() returns (uint256)"), view]
    /// Returns the total supply of the token.
    public fun total_supply(): U256 {
        u256_from_words(0,0)
    }

    #[callable(sig=b"balanceOf(address) returns (uint256)"), view]
    /// Returns the balance of an account.
    public fun balance_of(_owner: address): U256 {
        u256_from_words(0,0)
    }

    #[callable(sig=b"allowance(address, address) returns (uint256)"), view]
    /// Returns the allowance an account owner has approved for the given spender.
    public fun allowance(_owner: address, _spender: address): U256 {
        u256_from_words(0,0)
    }

    #[callable(sig=b"approve(address,uint256)returns(bool)")]
    /// Approves that the spender can spent the given amount on behalf of the calling account.
    public fun approve(_spender: address, _amount: U256): bool {
        true
    }

    #[callable(sig=b"transfer(address,uint256)returns (bool)")]
    /// Transfers the amount from the sending account to the given account
    public fun transfer(_to: address, _amount: U256): bool {
        true
    }

    #[callable(sig=b"transferFrom(address, address, uint256) returns(bool)")]
    /// Transfers the amount on behalf of the `from` account to the given account.
    /// This evaluates and adjusts the allowance.
    public fun transfer_from(_from: address, _to: address, _amount: U256): bool {
        true
    }


}
