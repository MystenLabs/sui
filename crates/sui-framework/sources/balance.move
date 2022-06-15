// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A storable handler for `Coin` balances.
/// Allows separation of the transferable `Coin` type and the storable
/// `Balance` eliminating the need to create new IDs for each application
/// that needs to hold coins.
module sui::balance {
    friend sui::coin;
    friend sui::sui_system;

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

    /// Can only be called by sui::coin.
    /// Create a `Balance` with a predefined value; required for minting new `Coin`s.
    public(friend) fun create_with_value<T>(value: u64): Balance<T> {
        Balance { value }
    }

    /// Can only be called by sui::coin.
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
module sui::balance_tests {
    use sui::balance;
    use sui::sui::SUI;

    #[test]
    fun test_balance() {
        let balance = balance::zero<SUI>();
        let another = balance::create_for_testing(1000);

        balance::join(&mut balance, another);

        assert!(balance::value(&balance) == 1000, 0);

        let balance1 = balance::split(&mut balance, 333);
        let balance2 = balance::split(&mut balance, 333);
        let balance3 = balance::split(&mut balance, 334);

        balance::destroy_zero(balance);

        assert!(balance::value(&balance1) == 333, 1);
        assert!(balance::value(&balance2) == 333, 2);
        assert!(balance::value(&balance3) == 334, 3);

        balance::destroy_for_testing(balance1);
        balance::destroy_for_testing(balance2);
        balance::destroy_for_testing(balance3);
    }
}
