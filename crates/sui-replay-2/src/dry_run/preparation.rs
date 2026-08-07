// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! State-independent preparation for locally simulating uncommitted transactions.
//!
//! This module normalizes mock gas and validates transaction data. Loading and checking
//! on-chain inputs, constructing gas status, and execution belong to later stages.

use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    digests::TransactionDigest,
    error::SuiResult,
    object::{MoveObject, OBJECT_START_VERSION, Object, Owner},
    transaction::{InputObjectKind, TransactionData, TransactionDataAPI, TxValidityCheckContext},
};

// Keep this in sync with `DEV_INSPECT_GAS_COIN_VALUE` in `sui-core/src/authority.rs`.
// The synthetic coin must cover every valid transaction gas budget.
const LOCAL_SIMULATION_GAS_COIN_VALUE: u64 = 1_000_000_000_000_000_000;

/// State-independent data produced before a local simulation loads transaction inputs.
#[derive(Debug)]
pub struct PreparedLocalSimulationTransaction {
    /// Transaction after synthetic gas normalization and full validity checks.
    pub transaction: TransactionData,
    /// Input descriptors from the transaction before synthetic gas was added.
    pub input_object_kinds: Vec<InputObjectKind>,
    /// Receiving references captured separately; checkpoint dry-run rejects them before loading.
    pub receiving_object_refs: Vec<ObjectRef>,
    /// Synthetic gas object to add directly to loaded inputs, if one was needed.
    pub mock_gas_object: Option<Object>,
}

/// Normalize and validate transaction data before loading inputs for a local simulation.
///
/// This mirrors the state-independent portion of `AuthorityState::simulate_transaction`.
/// Input descriptors are collected before mock gas is injected so callers never try to
/// fetch the synthetic object from storage. Full transaction validity runs afterward,
/// matching fullnode simulation's ordering.
pub fn prepare_transaction_for_local_simulation(
    mut transaction: TransactionData,
    validity_context: &TxValidityCheckContext<'_>,
    allow_mock_gas_coin: bool,
) -> SuiResult<PreparedLocalSimulationTransaction> {
    // Capture the original descriptors before adding mock gas. The mock object is
    // appended directly to loaded inputs by the later stateful preparation phase.
    let input_object_kinds = transaction.input_objects()?;
    let receiving_object_refs = transaction.receiving_objects();

    // Empty payment means either gasless execution or simulation with a synthetic coin.
    // Gasless transactions must retain their empty payment and zero-priced gas shape.
    let is_gasless =
        validity_context.config.enable_gasless() && transaction.is_gasless_transaction();
    let mock_gas_object = if allow_mock_gas_coin && transaction.gas().is_empty() && !is_gasless {
        let object = new_mock_gas_object(transaction.gas_owner());
        transaction.gas_data_mut().payment = vec![object.compute_object_reference()];
        Some(object)
    } else {
        None
    };

    transaction.validity_check(validity_context)?;

    Ok(PreparedLocalSimulationTransaction {
        transaction,
        input_object_kinds,
        receiving_object_refs,
        mock_gas_object,
    })
}

/// Construct the synthetic gas coin used by fullnode-style local simulation.
fn new_mock_gas_object(owner: SuiAddress) -> Object {
    Object::new_move(
        MoveObject::new_gas_coin(
            OBJECT_START_VERSION,
            ObjectID::MAX,
            LOCAL_SIMULATION_GAS_COIN_VALUE,
        ),
        Owner::AddressOwner(owner),
        TransactionDigest::genesis_marker(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::{
        base_types::SequenceNumber,
        digests::{ChainIdentifier, ObjectDigest},
        error::SuiErrorKind,
        transaction::{
            Argument, CallArg, Command, GasData, ObjectArg, ProgrammableTransaction,
            TransactionDataV1, TransactionExpiration, TransactionKind,
        },
    };

    const REFERENCE_GAS_PRICE: u64 = 1_000;

    fn object_ref() -> ObjectRef {
        (
            ObjectID::random(),
            SequenceNumber::from_u64(1),
            ObjectDigest::random(),
        )
    }

    fn programmable_transaction(inputs: Vec<CallArg>, commands: Vec<Command>) -> TransactionKind {
        TransactionKind::ProgrammableTransaction(ProgrammableTransaction { inputs, commands })
    }

    fn transaction(
        config: &ProtocolConfig,
        kind: TransactionKind,
        sender: SuiAddress,
        gas_owner: SuiAddress,
        payment: Vec<ObjectRef>,
        expiration: TransactionExpiration,
    ) -> TransactionData {
        TransactionData::new_with_gas_data_and_expiration(
            kind,
            sender,
            GasData {
                payment,
                owner: gas_owner,
                price: REFERENCE_GAS_PRICE,
                budget: config.max_tx_gas(),
            },
            expiration,
        )
    }

    fn validity_context(
        config: &ProtocolConfig,
        epoch: u64,
        chain_identifier: ChainIdentifier,
    ) -> TxValidityCheckContext<'_> {
        TxValidityCheckContext {
            config,
            epoch,
            chain_identifier,
            reference_gas_price: REFERENCE_GAS_PRICE,
        }
    }

    #[test]
    fn injects_mock_gas_after_collecting_input_descriptors() {
        let config = ProtocolConfig::get_for_max_version_UNSAFE();
        let chain_identifier = ChainIdentifier::default();
        let sender = SuiAddress::random_for_testing_only();
        let sponsor = SuiAddress::random_for_testing_only();
        let owned_ref = object_ref();
        let receiving_ref = object_ref();
        let transaction = transaction(
            &config,
            programmable_transaction(
                vec![
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(owned_ref)),
                    CallArg::Object(ObjectArg::Receiving(receiving_ref)),
                ],
                vec![],
            ),
            sender,
            sponsor,
            vec![],
            TransactionExpiration::None,
        );
        let original_digest = transaction.digest();

        let prepared = prepare_transaction_for_local_simulation(
            transaction,
            &validity_context(&config, 0, chain_identifier),
            true,
        )
        .unwrap();

        assert_eq!(
            prepared.input_object_kinds,
            vec![InputObjectKind::ImmOrOwnedMoveObject(owned_ref)]
        );
        assert_eq!(prepared.receiving_object_refs, vec![receiving_ref]);

        let mock_gas = prepared.mock_gas_object.as_ref().unwrap();
        assert_eq!(mock_gas.id(), ObjectID::MAX);
        assert_eq!(mock_gas.version(), OBJECT_START_VERSION);
        assert_eq!(mock_gas.owner(), &Owner::AddressOwner(sponsor));
        assert_eq!(
            mock_gas.as_inner().previous_transaction,
            TransactionDigest::genesis_marker()
        );
        let move_object = mock_gas.as_inner().data.try_as_move().unwrap();
        assert!(move_object.type_().is_gas_coin());
        assert_eq!(
            move_object.get_coin_value_unsafe(),
            LOCAL_SIMULATION_GAS_COIN_VALUE
        );
        assert_eq!(
            prepared.transaction.gas(),
            &[mock_gas.compute_object_reference()]
        );
        assert_ne!(prepared.transaction.digest(), original_digest);
    }

    #[test]
    fn preserves_an_explicit_gas_payment() {
        let config = ProtocolConfig::get_for_max_version_UNSAFE();
        let sender = SuiAddress::random_for_testing_only();
        let gas_ref = object_ref();
        let transaction = transaction(
            &config,
            programmable_transaction(vec![], vec![]),
            sender,
            sender,
            vec![gas_ref],
            TransactionExpiration::None,
        );

        let prepared = prepare_transaction_for_local_simulation(
            transaction.clone(),
            &validity_context(&config, 0, ChainIdentifier::default()),
            true,
        )
        .unwrap();

        assert_eq!(prepared.transaction, transaction);
        assert_eq!(
            prepared.input_object_kinds,
            vec![InputObjectKind::ImmOrOwnedMoveObject(gas_ref)]
        );
        assert!(prepared.receiving_object_refs.is_empty());
        assert!(prepared.mock_gas_object.is_none());
    }

    #[test]
    fn does_not_inject_mock_gas_for_gasless_transaction() {
        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        config.enable_gasless_for_testing();
        let sender = SuiAddress::random_for_testing_only();
        let coin_ref = object_ref();
        let transaction = TransactionData::V1(TransactionDataV1 {
            kind: programmable_transaction(
                vec![
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_ref)),
                    CallArg::Pure(bcs::to_bytes(&1u64).unwrap()),
                ],
                vec![Command::SplitCoins(
                    Argument::Input(0),
                    vec![Argument::Input(1)],
                )],
            ),
            sender,
            gas_data: GasData {
                payment: vec![],
                owner: sender,
                price: 0,
                budget: 0,
            },
            expiration: TransactionExpiration::None,
        });

        let prepared = prepare_transaction_for_local_simulation(
            transaction,
            &validity_context(&config, 0, ChainIdentifier::default()),
            true,
        )
        .unwrap();

        assert!(prepared.transaction.gas().is_empty());
        assert!(prepared.mock_gas_object.is_none());
        assert_eq!(
            prepared.input_object_kinds,
            vec![InputObjectKind::ImmOrOwnedMoveObject(coin_ref)]
        );
    }

    #[test]
    fn mock_gas_value_covers_the_maximum_budget() {
        let config = ProtocolConfig::get_for_max_version_UNSAFE();
        assert!(LOCAL_SIMULATION_GAS_COIN_VALUE >= config.max_tx_gas());
    }

    #[test]
    fn rejects_expired_transaction() {
        let config = ProtocolConfig::get_for_max_version_UNSAFE();
        let sender = SuiAddress::random_for_testing_only();
        let transaction = transaction(
            &config,
            programmable_transaction(vec![], vec![]),
            sender,
            sender,
            vec![object_ref()],
            TransactionExpiration::Epoch(6),
        );

        let error = prepare_transaction_for_local_simulation(
            transaction,
            &validity_context(&config, 7, ChainIdentifier::default()),
            true,
        )
        .unwrap_err();

        assert!(matches!(error.as_inner(), SuiErrorKind::TransactionExpired));
    }
}
