// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_field)]
/// [DEPRECATED]
/// This module is deprecated and is no longer used in the DeepBook codebase,
/// Use `custodian_v2` instead (paired with `clob_v2`).
///
/// Legacy type definitions and public functions are kept due to package upgrade
/// constraints.
module deepbook::custodian {
    use sui::table::Table;
    use sui::balance::Balance;
    use sui::object::{UID, ID};
    use sui::tx_context::TxContext;

    /// Deprecated methods.
    const EDeprecated: u64 = 1337;

    /// A single account stored in the `Custodian` object in the `account_balances`
    /// table.
    struct Account<phantom T> has store {
        available_balance: Balance<T>,
        locked_balance: Balance<T>,
    }

    struct AccountCap has key, store { id: UID }

    /// Custodian for limit orders.
    struct Custodian<phantom T> has key, store {
        id: UID,
        account_balances: Table<ID, Account<T>>,
    }

    /// Deprecated: use `custodian_v2::mint_account_cap` instead.
    public fun mint_account_cap(_ctx: &mut TxContext): AccountCap {
        abort EDeprecated
    }
}
