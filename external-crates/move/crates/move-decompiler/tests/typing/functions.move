module std::Functions {
    use std::signer;

    public fun f1() {
    }

    public fun f2(x: u8, y: u8): u8 {
        0
    }

    public fun f3(x: &u8, y: &mut u8, z: u8): u8 {
        *y = *x + z;
        *y
    }

    public fun f4(): (u8, u16, u32) {
        (8,16,32)
    }

    struct Coin<phantom CoinType> has store {
        value: u64
    }

    struct Coin2<phantom CoinType> has store {
        value: u64,
        value2: u64
    }

    struct Balance<phantom CoinType> has key {
        coin: Coin<CoinType>
    }

    /// Publish an empty balance resource under `account`'s address. This function must be called before
    /// minting or transferring to the account.
    public fun publish_balance<CoinType>(account: &signer): Coin<CoinType> {
        Coin<CoinType> { value: 0 }
    }

    /// Mint `amount` tokens to `mint_addr`. This method requires a witness with `CoinType` so that the
    /// module that owns `CoinType` can decide the minting policy.
    public fun mint<CoinType: drop>(mint_addr: address, amount: u64, _witness: CoinType) {
        // Deposit `total_value` amount of tokens to mint_addr's balance
        deposit(mint_addr, Coin<CoinType> { value: amount });
    }

    // public fun balance_of<CoinType>(owner: address): u64 {
    //     borrow_global<Balance<CoinType>>(owner).coin.value
    // }

    spec balance_of {
        pragma aborts_if_is_strict;
    }

    /// Transfers `amount` of tokens from `from` to `to`. This method requires a witness with `CoinType` so that the
    /// module that owns `CoinType` can  decide the transferring policy.
    public fun transfer<CoinType: drop>(from: &signer, to: address, amount: u64, _witness: CoinType) {
        let check = withdraw<CoinType>(signer::address_of(from), amount);
        deposit<CoinType>(to, check);
    }

    fun withdraw<CoinType>(addr: address, amount: u64) : Coin<CoinType> {
        // let balance = balance_of<CoinType>(addr);
        // assert!(balance >= amount, 1337);
        // let balance_ref = &mut borrow_global_mut<Balance<CoinType>>(addr).coin.value;
        // *balance_ref = balance - amount;
        Coin<CoinType> { value: amount }
    }

    fun deposit<CoinType>(addr: address, check: Coin<CoinType>): u64{
        // let balance = balance_of<CoinType>(addr);
        // let balance_ref = &mut borrow_global_mut<Balance<CoinType>>(addr).coin.value;
        let Coin { value } = check;
        // *balance_ref = balance + value;
        value
    }

    fun test_abort(): u8 {
        abort 2
    }
}
