// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A storable handler for `Coin` balances.
/// Allows separation of the transferable `Coin` type and the storable
/// `Balance` eliminating the need to create new IDs for each application
/// that needs to hold coins.
module Sui::Balance {
    friend Sui::Coin;
    friend Sui::SuiSystem;

    /// For when trying to destroy a non-zero balance.
    const ENonZero: u64 = 0;
    /// For when trying to withdraw more than there is.
    const ENotEnough: u64 = 0;

    /// Storable balance - an inner struct of a Coin type.
    /// Can be used to store coins which don't need to have the
    /// key ability.
    /// Helpful in representing a Coin without having to create a stand-alone object.
    struct Balance<phantom T> has store {
        value: u64
    }

    /// Get the amount stored in a `Balance`.
    public fun value<T>(self: &Balance<T>): u64 {
        self.value
    }

    /// Create a zero `Balance` for type `T`.
    public fun zero<T>(): Balance<T> {
        Balance { value: 0 }
    }

    /// Join two balances together.
    public fun join<T>(self: &mut Balance<T>, balance: Balance<T>) {
        let Balance { value } = balance;
        self.value = self.value + value;
    }

    /// Split a `Balance` and take a sub balance from it.
    public fun split<T>(self: &mut Balance<T>, value: u64): Balance<T> {
        assert!(self.value >= value, ENotEnough);
        self.value = self.value - value;
        Balance { value }
    }

    /// Destroy a zero `Balance`.
    public fun destroy_zero<T>(balance: Balance<T>) {
        assert!(balance.value == 0, ENonZero);
        let Balance { value: _ } = balance;
    }

    /// Can only be called by Sui::Coin.
    /// Create a `Balance` with a predefined value; required for minting new `Coin`s.
    public(friend) fun create_with_value<T>(value: u64): Balance<T> {
        Balance { value }
    }

    /// Can only be called by Sui::Coin.
    /// Destroy a `Balance` returning its value. Required for burning `Coin`s
    public(friend) fun destroy<T>(self: Balance<T>): u64 {
        let Balance { value } = self;
        value
    }

    #[test_only]
    /// Create a `Balance` of any coin for testing purposes.
    public fun create_for_testing<T>(value: u64): Balance<T> {
        create_with_value(value)
    }

    #[test_only]
    /// Destroy a `Balance` with any value in it for testing purposes.
    public fun destroy_for_testing<T>(self: Balance<T>): u64 {
        destroy(self)
    }
}

#[test_only]
module Sui::BalanceTests {
    use Sui::Balance;
    use Sui::SUI::SUI;

    #[test]
    fun test_balance() {
        let balance = Balance::zero<SUI>();
        let another = Balance::create_for_testing(1000);

        Balance::join(&mut balance, another);

        assert!(Balance::value(&balance) == 1000, 0);

        let balance1 = Balance::split(&mut balance, 333);
        let balance2 = Balance::split(&mut balance, 333);
        let balance3 = Balance::split(&mut balance, 334);

        Balance::destroy_zero(balance);

        assert!(Balance::value(&balance1) == 333, 1);
        assert!(Balance::value(&balance2) == 333, 2);
        assert!(Balance::value(&balance3) == 334, 3);

        Balance::destroy_for_testing(balance1);
        Balance::destroy_for_testing(balance2);
        Balance::destroy_for_testing(balance3);
    }
}
