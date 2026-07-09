// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{language_storage::TypeTag, u256::U256};
use sui_protocol_config::ProtocolConfig;

use crate::{
    accumulator_root::AccumulatorValue,
    allowance::Allowance,
    base_types::{ObjectID, SequenceNumber, SuiAddress, random_object_ref},
    coin_reservation::{CoinReservationResolverTrait, ParsedObjectRefWithdrawal},
    digests::{ChainIdentifier, CheckpointDigest, TransactionDigest},
    error::UserInputResult,
    gas_coin::GAS,
    id::UID,
    object::{MoveObject, OBJECT_START_VERSION, Object, Owner},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        CallArg, FundsWithdrawalArg, GasData, InputObjectKind, InputObjects, ObjectArg,
        ObjectReadResult, ProgrammableTransaction, SharedObjectMutability, TransactionData,
        TransactionDataAPI, TransactionDataV1, TransactionExpiration, TransactionKind,
        TxValidityCheckContext, WithdrawalTypeArg,
    },
};

fn protocol_config() -> ProtocolConfig {
    let mut cfg = ProtocolConfig::get_for_max_version_UNSAFE();
    cfg.enable_accumulators_for_testing();
    cfg
}

struct NoImpl;

impl CoinReservationResolverTrait for NoImpl {
    fn resolve_funds_withdrawal(
        &self,
        _: SuiAddress,
        _: ParsedObjectRefWithdrawal,
        _: Option<SequenceNumber>,
    ) -> UserInputResult<FundsWithdrawalArg> {
        unimplemented!("these tests do not use coin reservations")
    }
}

/// A mock resolver that always returns a SUI withdrawal with the parsed amount
struct MockSuiResolver;

impl CoinReservationResolverTrait for MockSuiResolver {
    fn resolve_funds_withdrawal(
        &self,
        _sender: SuiAddress,
        coin_reservation: ParsedObjectRefWithdrawal,
        _accumulator_version: Option<SequenceNumber>,
    ) -> UserInputResult<FundsWithdrawalArg> {
        Ok(FundsWithdrawalArg::balance_from_sender(
            coin_reservation.reservation_amount(),
            GAS::type_tag(),
        ))
    }
}

#[test]
fn test_withdraw_max_amount() {
    let arg = FundsWithdrawalArg::balance_from_sender(100, GAS::type_tag());
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.funds_withdrawal(arg.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.has_funds_withdrawals());
    let withdraws = tx
        .process_funds_withdrawals_for_signing(ChainIdentifier::default(), &NoImpl)
        .unwrap();
    let account_id = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(GAS::type_tag()).to_type_tag(),
    )
    .unwrap();
    assert_eq!(withdraws.len(), 1);
    assert_eq!(withdraws.get(&account_id).unwrap().0, 100);
}

#[test]
fn test_multiple_withdraws_same_account() {
    let arg1 = FundsWithdrawalArg::balance_from_sender(100, GAS::type_tag());
    let arg2 = FundsWithdrawalArg::balance_from_sender(200, GAS::type_tag());
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.funds_withdrawal(arg1.clone()).unwrap();
    ptb.funds_withdrawal(arg2.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.has_funds_withdrawals());
    let withdraws = tx
        .process_funds_withdrawals_for_signing(ChainIdentifier::default(), &NoImpl)
        .unwrap();
    let account_id = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(GAS::type_tag()).to_type_tag(),
    )
    .unwrap();
    assert_eq!(withdraws.len(), 1);
    assert_eq!(withdraws.get(&account_id).unwrap().0, 300);
}

#[test]
fn test_multiple_withdraws_different_accounts() {
    let arg1 = FundsWithdrawalArg::balance_from_sender(100, GAS::type_tag());
    let arg2 = FundsWithdrawalArg::balance_from_sender(200, TypeTag::Bool);
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.funds_withdrawal(arg1.clone()).unwrap();
    ptb.funds_withdrawal(arg2.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    assert!(tx.has_funds_withdrawals());
    let withdraws = tx
        .process_funds_withdrawals_for_signing(ChainIdentifier::default(), &NoImpl)
        .unwrap();
    let account_id1 = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(GAS::type_tag()).to_type_tag(),
    )
    .unwrap();
    let account_id2 = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(TypeTag::Bool).to_type_tag(),
    )
    .unwrap();
    assert_eq!(withdraws.len(), 2);
    assert_eq!(withdraws.get(&account_id1).unwrap().0, 100);
    assert_eq!(withdraws.get(&account_id2).unwrap().0, 200);
}

#[test]
fn test_withdraw_zero_amount() {
    let arg = FundsWithdrawalArg::balance_from_sender(0, GAS::type_tag());
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.funds_withdrawal(arg.clone()).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx = TransactionData::new_programmable(
        sender,
        vec![random_object_ref()],
        ptb.finish(),
        1_000_000,
        1000,
    );
    assert!(
        tx.validity_check(&TxValidityCheckContext::from_cfg_for_testing(
            &protocol_config()
        ))
        .is_err()
    );
}

#[test]
fn test_withdraw_too_many_withdraws() {
    let mut ptb = ProgrammableTransactionBuilder::new();
    for _ in 0..11 {
        ptb.funds_withdrawal(FundsWithdrawalArg::balance_from_sender(
            100,
            GAS::type_tag(),
        ))
        .unwrap();
    }
    let sender = SuiAddress::random_for_testing_only();
    let tx = TransactionData::new_programmable(
        sender,
        vec![random_object_ref()],
        ptb.finish(),
        1_000_000,
        1000,
    );
    assert!(
        tx.validity_check(&TxValidityCheckContext::from_cfg_for_testing(
            &protocol_config()
        ))
        .is_err()
    );
}

#[test]
fn test_withdraw_overflow() {
    let arg1 = FundsWithdrawalArg::balance_from_sender(u64::MAX, GAS::type_tag());
    let arg2 = FundsWithdrawalArg::balance_from_sender(u64::MAX, GAS::type_tag());
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.funds_withdrawal(arg1).unwrap();
    ptb.funds_withdrawal(arg2).unwrap();
    let sender = SuiAddress::random_for_testing_only();
    let tx =
        TransactionData::new_programmable(sender, vec![random_object_ref()], ptb.finish(), 1, 1);
    let result = tx.process_funds_withdrawals_for_signing(ChainIdentifier::default(), &NoImpl);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("overflow"),
        "Expected overflow error, got: {}",
        err
    );
}

#[test]
fn test_mixed_withdrawal_and_gas_payment_aggregation() {
    let mut cfg = protocol_config();
    cfg.enable_address_balance_gas_payments_for_testing();

    let sender = SuiAddress::random_for_testing_only();
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.funds_withdrawal(FundsWithdrawalArg::balance_from_sender(
        5000,
        GAS::type_tag(),
    ))
    .unwrap();

    let tx = TransactionData::V1(TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: 1000,
            budget: 10_000_000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 42,
        },
    });

    let withdraws = tx
        .process_funds_withdrawals_for_signing(ChainIdentifier::default(), &NoImpl)
        .unwrap();
    let account_id = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(GAS::type_tag()).to_type_tag(),
    )
    .unwrap();
    assert_eq!(withdraws.len(), 1);
    assert_eq!(withdraws.get(&account_id).unwrap().0, 5000 + 10_000_000);
}

#[test]
fn test_process_withdrawals_includes_implicit_gas() {
    let sender = SuiAddress::random_for_testing_only();
    let ptb = ProgrammableTransactionBuilder::new();

    let tx = TransactionData::V1(TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: 1000,
            budget: 10_000_000,
        },
        expiration: TransactionExpiration::None,
    });

    let withdrawals = tx
        .process_funds_withdrawals_for_signing(ChainIdentifier::default(), &NoImpl)
        .unwrap();

    let sui_account_id = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(GAS::type_tag()).to_type_tag(),
    )
    .unwrap();
    assert_eq!(withdrawals.len(), 1);
    assert_eq!(withdrawals.get(&sui_account_id).unwrap().0, 10_000_000);
}

/// Test that process_funds_withdrawals_for_signing() includes coin reservations in gas payment.
#[test]
fn test_process_withdrawals_includes_coin_reservations_in_gas() {
    let sender = SuiAddress::random_for_testing_only();
    let ptb = ProgrammableTransactionBuilder::new();
    let chain_id = ChainIdentifier::from(CheckpointDigest::default());

    let coin_reservation = ParsedObjectRefWithdrawal::new(ObjectID::random(), 0, 5000);
    let coin_reservation_ref = coin_reservation.encode(SequenceNumber::new(), chain_id);

    let tx = TransactionData::V1(TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas_data: GasData {
            payment: vec![coin_reservation_ref],
            owner: sender,
            price: 1000,
            budget: 10_000_000,
        },
        expiration: TransactionExpiration::None,
    });

    let withdrawals = tx
        .process_funds_withdrawals_for_signing(chain_id, &MockSuiResolver)
        .unwrap();

    let sui_account_id = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(GAS::type_tag()).to_type_tag(),
    )
    .unwrap();
    assert_eq!(withdrawals.len(), 1);
    assert_eq!(withdrawals.get(&sui_account_id).unwrap().0, 5000);
}

/// Test that process_funds_withdrawals_for_signing() includes coin reservations in PTB inputs.
#[test]
fn test_process_withdrawals_includes_coin_reservations_in_ptb_inputs() {
    let sender = SuiAddress::random_for_testing_only();
    let chain_id = ChainIdentifier::from(CheckpointDigest::default());

    let coin_reservation = ParsedObjectRefWithdrawal::new(ObjectID::random(), 0, 7500);
    let coin_reservation_ref = coin_reservation.encode(SequenceNumber::new(), chain_id);

    // Create a PTB with a coin reservation as an input object
    let pt = ProgrammableTransaction {
        inputs: vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(
            coin_reservation_ref,
        ))],
        commands: vec![],
    };

    let tx = TransactionData::V1(TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(pt),
        sender,
        gas_data: GasData {
            payment: vec![random_object_ref()],
            owner: sender,
            price: 1000,
            budget: 10_000_000,
        },
        expiration: TransactionExpiration::None,
    });

    let withdrawals = tx
        .process_funds_withdrawals_for_signing(chain_id, &MockSuiResolver)
        .unwrap();

    let sui_account_id = AccumulatorValue::get_field_id(
        sender,
        &WithdrawalTypeArg::Balance(GAS::type_tag()).to_type_tag(),
    )
    .unwrap();
    assert_eq!(withdrawals.len(), 1);
    assert_eq!(withdrawals.get(&sui_account_id).unwrap().0, 7500);
}

/// Test that validity_check() counts implicit gas budget in num_reservations.
/// max_withdraws is 10. If we have 10 explicit withdrawals + implicit gas,
/// the total should be 11, which should fail validation.
#[test]
fn test_validity_check_counts_implicit_gas_in_num_reservations() {
    let mut cfg = protocol_config();
    cfg.enable_address_balance_gas_payments_for_testing();

    let sender = SuiAddress::random_for_testing_only();
    let mut ptb = ProgrammableTransactionBuilder::new();

    // Add exactly max_withdraws (10) explicit withdrawals
    for _ in 0..10 {
        ptb.funds_withdrawal(FundsWithdrawalArg::balance_from_sender(
            100,
            GAS::type_tag(),
        ))
        .unwrap();
    }

    // Transaction also has implicit gas budget (via gas_data.payment = [])
    let tx = TransactionData::V1(TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: 1000,
            budget: 10_000_000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::from(CheckpointDigest::default()),
            nonce: 42,
        },
    });

    // Total reservations = 10 explicit + 1 implicit gas = 11
    // max_withdraws = 10, so this should FAIL
    let result = tx.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&cfg));
    assert!(
        result.is_err(),
        "Expected validation to fail because total reservations (10 explicit + 1 implicit gas = 11) \
         exceeds max_withdraws (10). Got: {:?}",
        result
    );
}

/// Test that validity_check() counts coin reservations in num_reservations.
/// max_withdraws is 10. If we have 11 coin reservations, validation should fail.
#[test]
fn test_validity_check_counts_coin_reservations_in_num_reservations() {
    let cfg = protocol_config();
    let sender = SuiAddress::random_for_testing_only();
    let ptb = ProgrammableTransactionBuilder::new();
    let chain_id = ChainIdentifier::from(CheckpointDigest::default());

    // Create 11 coin reservations (exceeds max_withdraws of 10)
    let coin_reservations: Vec<_> = (0..11)
        .map(|_| {
            let reservation = ParsedObjectRefWithdrawal::new(ObjectID::random(), 0, 1000);
            reservation.encode(SequenceNumber::new(), chain_id)
        })
        .collect();

    let tx = TransactionData::V1(TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas_data: GasData {
            payment: coin_reservations,
            owner: sender,
            price: 1000,
            budget: 10_000_000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: chain_id,
            nonce: 42,
        },
    });

    // Total reservations = 11 coin reservations
    // max_withdraws = 10, so this should FAIL
    let result = tx.validity_check(&TxValidityCheckContext::from_cfg_for_testing(&cfg));
    assert!(
        result.is_err(),
        "Expected validation to fail because 11 coin reservations exceeds max_withdraws (10). Got: {:?}",
        result
    );
}

/// A loaded-input set containing a real shared `Allowance<funds_type>` object,
/// as `read_objects_for_signing` would produce for a declared shared input.
fn allowance_input_objects(
    allowance: ObjectID,
    funder: SuiAddress,
    spender: SuiAddress,
    funds_type: TypeTag,
) -> InputObjects {
    let contents = Allowance {
        id: UID::new(allowance),
        funder,
        spender: Some(spender),
        app: None,
        lifetime_cap: None,
        current_spend: U256::zero(),
        start_timestamp_ms: None,
        expiration_timestamp_ms: None,
        rate_limit: None,
    };
    // SAFETY: `Allowance` is key-only, so it has no public transfer.
    let move_object = unsafe {
        MoveObject::new_from_execution_with_limit(
            Allowance::type_(funds_type).into(),
            false,
            OBJECT_START_VERSION,
            bcs::to_bytes(&contents).unwrap(),
            u64::MAX,
        )
        .unwrap()
    };
    let object = Object::new_move(
        move_object,
        Owner::Shared {
            initial_shared_version: OBJECT_START_VERSION,
        },
        TransactionDigest::genesis_marker(),
    );
    InputObjects::new(vec![ObjectReadResult::new(
        InputObjectKind::SharedMoveObject {
            id: allowance,
            initial_shared_version: OBJECT_START_VERSION,
            mutability: SharedObjectMutability::Mutable,
        },
        object.into(),
    )])
}

fn balance_gas_type_tag() -> TypeTag {
    WithdrawalTypeArg::Balance(GAS::type_tag()).to_type_tag()
}

/// A tx from `sender` withdrawing 100 GAS under `allowance`, declaring `funder`.
/// The allowance is included as a declared shared-object input, as `spend` requires.
fn allowance_tx(sender: SuiAddress, funder: SuiAddress, allowance: ObjectID) -> TransactionData {
    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.input(CallArg::Object(ObjectArg::SharedObject {
        id: allowance,
        initial_shared_version: SequenceNumber::new(),
        mutability: SharedObjectMutability::Mutable,
    }))
    .unwrap();
    ptb.funds_withdrawal(FundsWithdrawalArg::balance_from_allowance(
        100,
        GAS::type_tag(),
        funder,
        allowance,
    ))
    .unwrap();
    TransactionData::new_programmable(
        sender,
        vec![random_object_ref()],
        ptb.finish(),
        1_000_000,
        1000,
    )
}

#[test]
fn test_allowance_withdraw_reserves_against_funder() {
    let sender = SuiAddress::random_for_testing_only();
    let funder = SuiAddress::random_for_testing_only();
    let tx = allowance_tx(sender, funder, ObjectID::random());

    // The reservation is keyed by the declared funder's account, not the sender's.
    let withdraws = tx
        .process_funds_withdrawals_for_signing(ChainIdentifier::default(), &NoImpl)
        .unwrap();
    let funder_account = AccumulatorValue::get_field_id(funder, &balance_gas_type_tag()).unwrap();
    assert_eq!(withdraws.len(), 1);
    assert_eq!(withdraws.get(&funder_account).unwrap().0, 100);
}

#[test]
fn test_validate_allowance_withdrawals() {
    let sender = SuiAddress::random_for_testing_only();
    let funder = SuiAddress::random_for_testing_only();
    let allowance = ObjectID::random();
    let tx = allowance_tx(sender, funder, allowance);

    let ok = allowance_input_objects(allowance, funder, sender, balance_gas_type_tag());
    tx.validate_allowance_withdrawals(&ok).unwrap();

    let wrong_funder = allowance_input_objects(
        allowance,
        SuiAddress::random_for_testing_only(),
        sender,
        balance_gas_type_tag(),
    );
    let err = tx.validate_allowance_withdrawals(&wrong_funder).unwrap_err();
    assert!(err.to_string().contains("funder"), "{err}");

    let wrong_spender = allowance_input_objects(
        allowance,
        funder,
        SuiAddress::random_for_testing_only(),
        balance_gas_type_tag(),
    );
    let err = tx
        .validate_allowance_withdrawals(&wrong_spender)
        .unwrap_err();
    assert!(err.to_string().contains("spender"), "{err}");

    let wrong_type = allowance_input_objects(
        allowance,
        funder,
        sender,
        WithdrawalTypeArg::Balance(TypeTag::Bool).to_type_tag(),
    );
    let err = tx.validate_allowance_withdrawals(&wrong_type).unwrap_err();
    assert!(err.to_string().contains("is for"), "{err}");

    // The allowance is missing from the loaded inputs (e.g. revoked).
    let err = tx
        .validate_allowance_withdrawals(&InputObjects::new(vec![]))
        .unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");

    // The input with the declared id is not an `Allowance`.
    let coin = Object::with_id_owner_for_testing(allowance, sender);
    let coin_ref = coin.compute_object_reference();
    let not_an_allowance = InputObjects::new(vec![ObjectReadResult::new(
        InputObjectKind::ImmOrOwnedMoveObject(coin_ref),
        coin.into(),
    )]);
    let err = tx
        .validate_allowance_withdrawals(&not_an_allowance)
        .unwrap_err();
    assert!(err.to_string().contains("not a sui::allowance"), "{err}");
}

#[test]
fn test_allowance_requires_feature_flag() {
    // Accumulators on, allowances off (mainnet's config): the variant must be
    // rejected at validity.
    let mut cfg = protocol_config();
    cfg.disable_allowances_for_testing();
    let sender = SuiAddress::random_for_testing_only();
    let funder = SuiAddress::random_for_testing_only();
    let tx = allowance_tx(sender, funder, ObjectID::random());

    let err = tx
        .validity_check(&TxValidityCheckContext::from_cfg_for_testing(&cfg))
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("Allowance withdrawals are not enabled"),
        "{err}"
    );
}

