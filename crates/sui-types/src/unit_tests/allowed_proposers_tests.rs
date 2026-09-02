// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    base_types::SuiAddress,
    digests::{ChainIdentifier, CheckpointDigest},
    error::UserInputError,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        AllowedProposers, GasData, TransactionDataV1, TransactionExpiration, TransactionKind,
    },
};
use sui_protocol_config::ProtocolConfig;

/// The reference gas price used by `TxValidityCheckContext::from_cfg_for_testing`.
const RGP: u64 = 1000;
/// The committee size used by `TxValidityCheckContext::from_cfg_for_testing`.
const COMMITTEE_SIZE: u32 = 4;
/// The epoch used by `TxValidityCheckContext::from_cfg_for_testing`.
const EPOCH: EpochId = 0;

fn allowed(proposers: Vec<u32>) -> AllowedProposers {
    allowed_at_epoch(proposers, EPOCH)
}

fn allowed_at_epoch(proposers: Vec<u32>, epoch: EpochId) -> AllowedProposers {
    AllowedProposers {
        epoch,
        proposers: NonEmpty::from_vec(proposers).expect("test proposer sets are non-empty"),
    }
}

fn tx_with_expiration(expiration: TransactionExpiration, gas_price: u64) -> TransactionDataV1 {
    let sender = SuiAddress::random_for_testing_only();
    TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(
            ProgrammableTransactionBuilder::new().finish(),
        ),
        sender,
        gas_data: GasData {
            payment: vec![random_object_ref()],
            owner: sender,
            price: gas_price,
            budget: gas_price * 1_000,
        },
        expiration,
    }
}

fn validity_expiration(allowed_proposers: Option<AllowedProposers>) -> TransactionExpiration {
    TransactionExpiration::Validity {
        min_epoch: Some(0),
        max_epoch: Some(0),
        min_timestamp: None,
        max_timestamp: None,
        chain: ChainIdentifier::from(CheckpointDigest::default()),
        nonce: 123,
        allowed_proposers,
    }
}

fn tx_with_allowed_proposers(proposers: Vec<u32>) -> TransactionDataV1 {
    tx_with_allowed_proposers_at_price(proposers, RGP)
}

fn tx_with_allowed_proposers_at_price(proposers: Vec<u32>, gas_price: u64) -> TransactionDataV1 {
    tx_with_expiration(validity_expiration(Some(allowed(proposers))), gas_price)
}

fn enabled_config() -> ProtocolConfig {
    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
    config.set_allowed_proposers_for_testing(true);
    config
}

fn assert_invalid_expiration(err: SuiError) {
    assert!(
        matches!(
            err.into_inner(),
            SuiErrorKind::UserInputError {
                error: UserInputError::InvalidExpiration { .. }
            }
        ),
        "expected InvalidExpiration"
    );
}

/// The variant is gated whether or not it carries a usable proposer set, so that nothing accepted
/// after the upgrade could have been accepted before it.
#[test]
fn test_validity_expiration_requires_feature_flag() {
    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
    config.set_allowed_proposers_for_testing(false);

    for expiration in [
        validity_expiration(Some(allowed(vec![0, 3]))),
        validity_expiration(Some(allowed_at_epoch(vec![0, 3], EPOCH + 1))),
        validity_expiration(None),
    ] {
        let err = tx_with_expiration(expiration.clone(), RGP)
            .validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config))
            .unwrap_err();
        assert!(
            matches!(
                err.into_inner(),
                SuiErrorKind::UserInputError {
                    error: UserInputError::Unsupported(_)
                }
            ),
            "expected Unsupported when allowed_proposers is disabled"
        );

        let enabled = enabled_config();
        tx_with_expiration(expiration, RGP)
            .validity_check(&TxValidityCheckContext::from_cfg_for_testing(&enabled))
            .unwrap();
    }
}

#[test]
fn test_allowed_proposers_must_be_strictly_increasing() {
    let config = enabled_config();

    for proposers in [vec![1, 1], vec![2, 1]] {
        let err = tx_with_allowed_proposers(proposers.clone())
            .validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config))
            .unwrap_err();
        assert_invalid_expiration(err);
    }

    tx_with_allowed_proposers(vec![0, 1, 3])
        .validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config))
        .unwrap();
}

/// SIP-45: naming more proposers than `MAX_UNPAID_ALLOWED_PROPOSERS` requires a gas price
/// above RGP, since each additional proposer amplifies the transaction's consensus cost.
#[test]
fn test_allowed_proposers_bounded_by_gas_price() {
    let config = enabled_config();

    // Large enough that the committee bound never fires ahead of the gas price bound.
    let context = TxValidityCheckContext {
        committee_size: 16,
        ..TxValidityCheckContext::from_cfg_for_testing(&config)
    };
    let check = |proposers: usize, gas_price: u64| {
        let tx =
            tx_with_allowed_proposers_at_price((0..proposers as u32).collect(), gas_price * RGP);
        tx.validity_check(&context)
    };

    // At RGP the unpaid allowance applies, regardless of the smaller gas_price / RGP.
    for proposers in 1..=MAX_UNPAID_ALLOWED_PROPOSERS as usize {
        check(proposers, 1).unwrap();
    }
    assert_invalid_expiration(check(MAX_UNPAID_ALLOWED_PROPOSERS as usize + 1, 1).unwrap_err());

    // Above the unpaid allowance, each additional proposer must be paid for.
    check(6, 6).unwrap();
    assert!(check(7, 6).is_err());
}

/// An index outside the committee names no validator, so it can only render the transaction
/// unproposable.
#[test]
fn test_allowed_proposers_bounded_by_committee_size() {
    let config = enabled_config();

    tx_with_allowed_proposers(vec![COMMITTEE_SIZE - 1])
        .validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config))
        .unwrap();

    let err = tx_with_allowed_proposers(vec![COMMITTEE_SIZE])
        .validity_check(&TxValidityCheckContext::from_cfg_for_testing(&config))
        .unwrap_err();
    assert_invalid_expiration(err);

    // No gas price buys more proposers than there are validators, so the whole committee is the
    // most that can ever be named.
    let context = TxValidityCheckContext::from_cfg_for_testing(&config);
    tx_with_allowed_proposers_at_price((0..COMMITTEE_SIZE).collect(), 100 * RGP)
        .validity_check(&context)
        .unwrap();

    let err = tx_with_allowed_proposers_at_price((0..COMMITTEE_SIZE + 1).collect(), 100 * RGP)
        .validity_check(&context)
        .unwrap_err();
    assert_invalid_expiration(err);
}

/// A proposer set recorded for another epoch indexes into a committee that is not deciding this
/// transaction, so it is ignored entirely — including by the checks that would otherwise reject it.
#[test]
fn test_stale_epoch_proposers_are_ignored() {
    let config = enabled_config();
    let context = TxValidityCheckContext::from_cfg_for_testing(&config);

    // Every rule a current-epoch set would violate: unsorted, out of committee, and more
    // proposers than the gas price pays for.
    for proposers in [
        vec![2, 1],
        vec![COMMITTEE_SIZE + 10],
        (0..MAX_UNPAID_ALLOWED_PROPOSERS as u32 + 5).collect(),
    ] {
        let expiration = validity_expiration(Some(allowed_at_epoch(proposers, EPOCH + 1)));
        tx_with_expiration(expiration.clone(), RGP)
            .validity_check(&context)
            .unwrap();

        // ...and the transaction is proposable by anyone, as if it named no proposers.
        assert!(expiration.is_allowed_proposer(0, EPOCH));
        assert!(expiration.is_allowed_proposer(u32::MAX, EPOCH));
        assert!(!expiration.restricts_proposers(EPOCH));
        assert!(expiration.allowed_proposers(EPOCH).is_none());

        // At the epoch it was recorded for, the set applies again.
        assert!(expiration.restricts_proposers(EPOCH + 1));
    }
}

#[test]
fn test_validity_expiration_checks_epoch_range() {
    let config = enabled_config();

    let mut tx = tx_with_allowed_proposers(vec![0]);
    let TransactionExpiration::Validity { max_epoch, .. } = &mut tx.expiration else {
        panic!("expected Validity expiration");
    };
    *max_epoch = Some(0);

    let context = TxValidityCheckContext {
        epoch: 1,
        ..TxValidityCheckContext::from_cfg_for_testing(&config)
    };
    assert!(matches!(
        tx.validity_check(&context).unwrap_err().into_inner(),
        SuiErrorKind::TransactionExpired
    ));
}

#[test]
fn test_is_allowed_proposer() {
    let expiration = validity_expiration(Some(allowed(vec![1, 3])));
    assert!(!expiration.is_allowed_proposer(0, EPOCH));
    assert!(expiration.is_allowed_proposer(1, EPOCH));
    assert!(!expiration.is_allowed_proposer(2, EPOCH));
    assert!(expiration.is_allowed_proposer(3, EPOCH));
    // Past the end of the set, where the sorted early-exit does not fire.
    assert!(!expiration.is_allowed_proposer(4, EPOCH));

    // Variants without a proposer restriction allow any proposer.
    assert!(validity_expiration(None).is_allowed_proposer(7, EPOCH));
    assert!(TransactionExpiration::None.is_allowed_proposer(7, EPOCH));
    assert!(TransactionExpiration::Epoch(1).is_allowed_proposer(7, EPOCH));
}

#[test]
fn test_validity_expiration_is_replay_protected() {
    let make = |min_epoch, max_epoch| TransactionExpiration::Validity {
        min_epoch: Some(min_epoch),
        max_epoch: Some(max_epoch),
        min_timestamp: None,
        max_timestamp: None,
        chain: ChainIdentifier::default(),
        nonce: 0,
        allowed_proposers: Some(allowed(vec![0])),
    };
    assert!(make(5, 5).is_replay_protected());
    assert!(make(5, 6).is_replay_protected());
    assert!(!make(5, 7).is_replay_protected());
}
