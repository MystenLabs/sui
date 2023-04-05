module NamedAddr::BasicCoin {
    struct Coin has store {
        value: u64
    }

    struct Balance has key {
        coin: Coin
    }

    /// Publish an empty balance resource under `account`'s address. This function must be called before
    /// minting or transferring to the account.
    public fun publish_balance(account: &signer) { .. }

    /// Mint `amount` tokens to `mint_addr`. Mint must be approved by the module owner.
    public fun mint(module_owner: &signer, mint_addr: address, amount: u64) acquires Balance { .. }

    /// Returns the balance of `owner`.
    public fun balance_of(owner: address): u64 acquires Balance { .. }

    /// Transfers `amount` of tokens from `from` to `to`.
    public fun transfer(from: &signer, to: address, amount: u64) acquires Balance { .. }
}
