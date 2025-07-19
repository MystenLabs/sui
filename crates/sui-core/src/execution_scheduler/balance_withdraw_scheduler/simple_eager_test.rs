// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use crate::execution_scheduler::balance_withdraw_scheduler::eager_scheduler::*;
    use sui_types::{base_types::SequenceNumber, transaction::Reservation};

    #[test]
    fn test_account_state_basic() {
        let mut state = AccountState::new(1000, SequenceNumber::from_u64(0));

        // Test basic reservation
        assert_eq!(state.minimum_guaranteed_balance(), 1000);
        assert!(state.try_reserve(&Reservation::MaxAmountU64(500)));
        assert_eq!(state.minimum_guaranteed_balance(), 500);
        assert!(state.try_reserve(&Reservation::MaxAmountU64(400)));
        assert_eq!(state.minimum_guaranteed_balance(), 100);
        assert!(!state.try_reserve(&Reservation::MaxAmountU64(200)));
        assert_eq!(state.minimum_guaranteed_balance(), 100);
    }

    #[test]
    fn test_account_state_entire_balance() {
        let mut state = AccountState::new(1000, SequenceNumber::from_u64(0));

        // Reserve entire balance
        assert!(state.try_reserve(&Reservation::EntireBalance));
        assert_eq!(state.minimum_guaranteed_balance(), 0);

        // Cannot reserve anything after entire balance
        assert!(!state.try_reserve(&Reservation::MaxAmountU64(1)));
        assert!(!state.try_reserve(&Reservation::EntireBalance));
    }

    #[test]
    fn test_account_state_settlement() {
        let mut state = AccountState::new(1000, SequenceNumber::from_u64(0));

        // Make some reservations
        assert!(state.try_reserve(&Reservation::MaxAmountU64(900)));
        assert_eq!(state.minimum_guaranteed_balance(), 100);

        // Apply settlement (simulating actual withdrawal of 900)
        state.apply_settlement(100, SequenceNumber::from_u64(1));

        // After settlement, reservations are cleared
        assert_eq!(state.minimum_guaranteed_balance(), 100);
        assert!(state.try_reserve(&Reservation::MaxAmountU64(100)));
        assert_eq!(state.minimum_guaranteed_balance(), 0);
    }
}
