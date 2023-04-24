// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_universe::{
        account::{AccountCurrent, AccountData},
        default_num_accounts, default_num_transactions,
        helpers::{pick_slice_idxs, Index},
    },
    executor::Executor,
};

use proptest::{
    collection::{vec, SizeRange},
    prelude::*,
};
use proptest_derive::Arbitrary;

const PICK_SIZE: usize = 3;

/// A set of accounts which can be used to construct an initial state.
#[derive(Debug)]
pub struct AccountUniverseGen {
    accounts: Vec<AccountData>,
    pick_style: AccountPickStyle,
}

/// A set of accounts that has been set up and can now be used to conduct transactions on.
#[derive(Clone, Debug)]
pub struct AccountUniverse {
    accounts: Vec<AccountCurrent>,
    picker: AccountPicker,
    /// Whether to ignore any new accounts that transactions add to the universe.
    ignore_new_accounts: bool,
}

/// Allows accounts to be randomly selected from an account universe.
#[derive(Arbitrary, Clone, Debug)]
pub struct AccountPairGen {
    indices: [Index; PICK_SIZE],
    // The pick_slice_idx method used by this struct returns values in order, so use this flag
    // to determine whether to reverse it.
    reverse: bool,
}

/// Determines the sampling algorithm used to pick accounts from the universe.
#[derive(Clone, Debug)]
pub enum AccountPickStyle {
    /// An account may be picked as many times as possible.
    Unlimited,
    /// An account may only be picked these many times.
    Limited(usize),
}

#[derive(Clone, Debug)]
enum AccountPicker {
    Unlimited(usize),
    // Vector of (index, times remaining).
    Limited(Vec<(usize, usize)>),
}

impl AccountUniverseGen {
    /// Returns a [`Strategy`] that generates a universe of accounts with pre-populated initial
    /// balances.
    pub fn strategy(
        num_accounts: impl Into<SizeRange>,
        balance_strategy: impl Strategy<Value = u64>,
    ) -> impl Strategy<Value = Self> {
        // Pick a sequence number in a smaller range so that valid transactions can be generated.
        // XXX should we also test edge cases around large sequence numbers?
        // Note that using a function as a strategy directly means that shrinking will not occur,
        // but that should be fine because there's nothing to really shrink within accounts anyway.
        vec(AccountData::strategy(balance_strategy), num_accounts).prop_map(|accounts| Self {
            accounts,
            pick_style: AccountPickStyle::Unlimited,
        })
    }

    /// Returns a [`Strategy`] that generates a universe of accounts that's guaranteed to succeed,
    /// assuming that any transfers out of accounts will be 100_000 or below.
    pub fn success_strategy(min_accounts: usize) -> impl Strategy<Value = Self> {
        // Set the minimum balance to be 5x possible transfers out to handle potential gas cost
        // issues.
        let min_balance = (100_000 * (default_num_transactions()) * 5) as u64;
        let max_balance = min_balance * 10;
        Self::strategy(
            min_accounts..default_num_accounts(),
            min_balance..max_balance,
        )
    }

    /// Sets the pick style used by this account universe.
    pub fn set_pick_style(&mut self, pick_style: AccountPickStyle) -> &mut Self {
        self.pick_style = pick_style;
        self
    }

    /// Returns the number of accounts in this account universe.
    pub fn num_accounts(&self) -> usize {
        self.accounts.len()
    }

    /// Returns an [`AccountUniverse`] with the initial state generated in this universe.
    pub fn setup(self, executor: &mut Executor) -> AccountUniverse {
        for account_data in &self.accounts {
            executor.add_objects(&account_data.coins);
        }

        AccountUniverse::new(self.accounts, self.pick_style, false)
    }
}

impl AccountUniverse {
    fn new(
        accounts: Vec<AccountData>,
        pick_style: AccountPickStyle,
        ignore_new_accounts: bool,
    ) -> Self {
        let accounts: Vec<_> = accounts.into_iter().map(AccountCurrent::new).collect();
        let picker = AccountPicker::new(pick_style, accounts.len());

        Self {
            accounts,
            picker,
            ignore_new_accounts,
        }
    }

    /// Returns the number of accounts currently in this universe.
    ///
    /// Some transactions might cause new accounts to be created. The return value of this method
    /// will include those new accounts.
    pub fn num_accounts(&self) -> usize {
        self.accounts.len()
    }

    /// Returns the accounts currently in this universe.
    ///
    /// Some transactions might cause new accounts to be created. The return value of this method
    /// will include those new accounts.
    pub fn accounts(&self) -> &[AccountCurrent] {
        &self.accounts
    }

    /// Adds an account to the universe so that future transactions can be made out of this account.
    ///
    /// This is ignored if the universe was configured to be in gas-cost-stability mode.
    pub fn add_account(&mut self, account_data: AccountData) {
        if !self.ignore_new_accounts {
            self.accounts.push(AccountCurrent::new(account_data));
        }
    }

    /// Picks an account using the provided `Index` as a source of randomness.
    pub fn pick(&mut self, index: Index) -> (usize, &mut AccountCurrent) {
        let idx = self.picker.pick(index);
        (idx, &mut self.accounts[idx])
    }
}

impl AccountPicker {
    fn new(pick_style: AccountPickStyle, num_accounts: usize) -> Self {
        match pick_style {
            AccountPickStyle::Unlimited => AccountPicker::Unlimited(num_accounts),
            AccountPickStyle::Limited(limit) => {
                let remaining = (0..num_accounts).map(|idx| (idx, limit)).collect();
                AccountPicker::Limited(remaining)
            }
        }
    }

    fn pick(&mut self, index: Index) -> usize {
        match self {
            AccountPicker::Unlimited(num_accounts) => index.index(*num_accounts),
            AccountPicker::Limited(remaining) => {
                let remaining_idx = index.index(remaining.len());
                Self::pick_limited(remaining, remaining_idx)
            }
        }
    }

    fn pick_account_indices(&mut self, indexes: &[Index; PICK_SIZE]) -> [usize; PICK_SIZE] {
        match self {
            AccountPicker::Unlimited(num_accounts) => {
                Self::pick_account_indices_impl(*num_accounts, indexes)
            }
            AccountPicker::Limited(remaining) => {
                Self::pick_account_indices_impl(remaining.len(), indexes).map(|idx| {
                    let (account_idx, _) = remaining[idx];
                    account_idx
                })
            }
        }
    }

    fn pick_account_indices_impl(max: usize, indexes: &[Index; PICK_SIZE]) -> [usize; PICK_SIZE] {
        let idxs = pick_slice_idxs(max, indexes);
        assert_eq!(idxs.len(), PICK_SIZE);
        let idxs: [usize; PICK_SIZE] = idxs[0..PICK_SIZE].try_into().unwrap();
        assert!(
            idxs[0] < idxs[1],
            "pick_slice_idxs should return sorted order"
        );
        idxs
    }

    fn pick_limited(remaining: &mut Vec<(usize, usize)>, remaining_idx: usize) -> usize {
        let (account_idx, times_remaining) = {
            let (account_idx, times_remaining) = &mut remaining[remaining_idx];
            *times_remaining -= 1;
            (*account_idx, *times_remaining)
        };

        if times_remaining == 0 {
            // Remove the account from further consideration.
            remaining.remove(remaining_idx);
        }

        account_idx
    }
}

impl AccountPairGen {
    /// Picks two accounts randomly from this universe and returns mutable references to
    /// them.
    pub fn pick<'a>(&self, universe: &'a mut AccountUniverse) -> AccountTriple<'a> {
        let [low_idx, mid_idx, high_idx] = universe.picker.pick_account_indices(&self.indices);
        // Need to use `split_at_mut` because you can't have multiple mutable references to items
        // from a single slice at any given time.
        let (head, tail) = universe.accounts.split_at_mut(low_idx + 1);
        let (mid, tail) = tail.split_at_mut(mid_idx - low_idx);
        let (low_account, mid_account, high_account) = (
            head.last_mut().unwrap(),
            mid.last_mut().unwrap(),
            tail.last_mut().unwrap(),
        );

        if self.reverse {
            AccountTriple {
                idx_1: high_idx,
                idx_2: mid_idx,
                idx_3: low_idx,
                account_1: high_account,
                account_2: mid_account,
                account_3: low_account,
            }
        } else {
            AccountTriple {
                idx_1: low_idx,
                idx_2: mid_idx,
                idx_3: high_idx,
                account_1: low_account,
                account_2: mid_account,
                account_3: high_account,
            }
        }
    }
}

/// Mutable references to a three-tuple of distinct accounts picked from a universe.
pub struct AccountTriple<'a> {
    /// The index of the first account picked.
    pub idx_1: usize,
    /// The index of the second account picked.
    pub idx_2: usize,
    /// The index of the third account picked.
    pub idx_3: usize,
    /// A mutable reference to the first account picked.
    pub account_1: &'a mut AccountCurrent,
    /// A mutable reference to the second account picked.
    pub account_2: &'a mut AccountCurrent,
    /// A mutable reference to the third account picked.
    pub account_3: &'a mut AccountCurrent,
}
