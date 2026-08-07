// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Checkpoint-bound input loading and canonical checks for local simulation.

use super::prepare_transaction_for_local_simulation;
use anyhow::{Context, Result, bail};
use std::{collections::BTreeMap, sync::Arc};
use sui_config::verifier_signing_config::VerifierSigningConfig;
use sui_data_store::{
    CheckpointExecutionContext, CheckpointObjectRequest, CheckpointObjectSelector,
    CheckpointObjectStore,
};
use sui_types::{
    base_types::ObjectID,
    coin_reservation::ParsedDigest,
    digests::TransactionDigest,
    error::{SuiError, SuiErrorKind, UserInputError},
    gas::SuiGasStatus,
    metrics::BytecodeVerifierMetrics,
    object::Object,
    supported_protocol_versions::{ProtocolConfig, ProtocolVersion},
    transaction::{
        CheckedInputObjects, InputObjectKind, InputObjects, ObjectReadResult, ReceivingObjects,
        TransactionData, TransactionDataAPI, TxValidityCheckContext,
    },
};

/// A transaction prepared for checked execution against one finalized checkpoint.
pub struct PreparedLocalDryRun {
    /// Coherent chain, checkpoint, and epoch metadata used for every state read.
    pub(super) snapshot: CheckpointExecutionContext,
    /// Exact protocol configuration used for validity, input checks, and later execution.
    pub(super) protocol_config: ProtocolConfig,
    /// Transaction after mock-gas normalization and full validity checks.
    pub(super) transaction: TransactionData,
    /// Digest of the normalized transaction.
    pub(super) digest: TransactionDigest,
    /// Inputs returned by canonical transaction checks.
    pub(super) checked_inputs: CheckedInputObjects,
    /// Gas status returned by canonical transaction checks.
    pub(super) gas_status: SuiGasStatus,
    /// Deduplicated checkpoint-end object bodies collected while checking inputs.
    pub(super) checkpoint_objects: BTreeMap<ObjectID, Object>,
    /// Synthetic local gas object, when mock gas was injected.
    pub(super) mock_gas_object: Option<Object>,
}

/// Load the transaction's declared objects at the supplied checkpoint context and apply
/// canonical checks.
///
/// This stage does not execute the transaction or perform runtime object reads. Every loaded
/// object retains the descriptor supplied by the transaction so canonical digest, ownership,
/// shared-object, duplicate-input, replay-protection, and gas checks observe the original input.
/// When `allow_mock_gas_coin` is true, empty non-gasless payment is normalized to synthetic gas
/// rather than treated as address-balance payment.
pub fn prepare_checked_local_dry_run<S>(
    transaction: TransactionData,
    snapshot: CheckpointExecutionContext,
    store: &S,
    allow_mock_gas_coin: bool,
) -> Result<PreparedLocalDryRun>
where
    S: CheckpointObjectStore + ?Sized,
{
    if transaction.kind().is_system_tx() {
        return Err(unsupported_error(
            "simulate does not support system transactions",
        ));
    }

    let protocol_config = protocol_config_for_snapshot(&snapshot)?;
    if !protocol_config.simplified_unwrap_then_delete() {
        return Err(unsupported_error(format!(
            concat!(
                "local checkpoint dry-run does not support protocol version {} because it ",
                "requires deprecated parent-entry reads",
            ),
            protocol_config.version.as_u64(),
        )));
    }
    let validity_context = TxValidityCheckContext {
        config: &protocol_config,
        epoch: snapshot.epoch.epoch_id,
        chain_identifier: snapshot.chain_identifier,
        reference_gas_price: snapshot.epoch.rgp,
    };
    let prepared = prepare_transaction_for_local_simulation(
        transaction,
        &validity_context,
        allow_mock_gas_coin,
    )?;

    reject_unsupported_shapes(&prepared, &protocol_config)?;

    let requests = prepared
        .input_object_kinds
        .iter()
        .map(checkpoint_request)
        .collect::<Vec<_>>();
    let loaded_objects = if requests.is_empty() {
        Vec::new()
    } else {
        store.get_checkpoint_objects(&snapshot, &requests)?
    };

    let mut checkpoint_object_answers = Vec::with_capacity(requests.len());
    let mut input_objects = Vec::with_capacity(
        prepared.input_object_kinds.len() + usize::from(prepared.mock_gas_object.is_some()),
    );
    for (kind, object) in std::iter::zip(&prepared.input_object_kinds, loaded_objects) {
        let object = object.ok_or_else(|| missing_object_error(kind, &snapshot))?;
        validate_loaded_input(kind, &object, &snapshot)?;
        checkpoint_object_answers.push(object.clone());
        input_objects.push(ObjectReadResult::new(*kind, object.into()));
    }

    let mock_gas_id = prepared.mock_gas_object.as_ref().map(|object| object.id());
    if let Some(mock_gas) = &prepared.mock_gas_object {
        input_objects.push(ObjectReadResult::new_from_gas_object(mock_gas));
    }

    let registry = prometheus::Registry::new();
    let verifier_metrics = Arc::new(BytecodeVerifierMetrics::new(&registry));
    let verifier_signing_config = VerifierSigningConfig::default();
    let (gas_status, checked_inputs) = sui_transaction_checks::check_transaction_input(
        &protocol_config,
        snapshot.epoch.rgp,
        &prepared.transaction,
        InputObjects::new(input_objects),
        &ReceivingObjects::from(Vec::new()),
        &verifier_metrics,
        &verifier_signing_config,
    )?;
    validate_live_owned_inputs(
        store,
        &snapshot,
        &checked_inputs,
        mock_gas_id,
        &mut checkpoint_object_answers,
    )?;
    let checkpoint_objects = checkpoint_object_answers
        .into_iter()
        .map(|object| (object.id(), object))
        .collect();
    let digest = prepared.transaction.digest();

    Ok(PreparedLocalDryRun {
        snapshot,
        protocol_config,
        transaction: prepared.transaction,
        digest,
        checked_inputs,
        gas_status,
        checkpoint_objects,
        mock_gas_object: prepared.mock_gas_object,
    })
}

/// Recheck address-owned inputs against the latest live state at the selected checkpoint.
fn validate_live_owned_inputs<S>(
    store: &S,
    snapshot: &CheckpointExecutionContext,
    checked_inputs: &CheckedInputObjects,
    mock_gas_id: Option<ObjectID>,
    checkpoint_object_answers: &mut Vec<Object>,
) -> Result<()>
where
    S: CheckpointObjectStore + ?Sized,
{
    const LIVE_STATE_LAG_NOTE: &str =
        "note: the object reference may be newer than the state indexed by the GraphQL endpoint";

    let owned_refs = checked_inputs
        .inner()
        .filter_owned_objects()
        .into_iter()
        .filter(|object_ref| Some(object_ref.0) != mock_gas_id)
        .collect::<Vec<_>>();
    if owned_refs.is_empty() {
        return Ok(());
    }

    let requests = owned_refs
        .iter()
        .map(|object_ref| CheckpointObjectRequest {
            object_id: object_ref.0,
            selector: CheckpointObjectSelector::Latest,
        })
        .collect::<Vec<_>>();
    let live_objects = store.get_checkpoint_objects(snapshot, &requests)?;

    for (provided_ref, live_object) in std::iter::zip(owned_refs, live_objects) {
        let live_object = live_object
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "owned input {} at version {} is not live at checkpoint {}",
                    provided_ref.0,
                    provided_ref.1,
                    snapshot.checkpoint,
                )
            })
            .context(LIVE_STATE_LAG_NOTE)?;
        if live_object.version() != provided_ref.1 {
            return Err(anyhow::Error::new(SuiError::from(
                UserInputError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: provided_ref,
                    current_version: live_object.version(),
                },
            ))
            .context(LIVE_STATE_LAG_NOTE));
        }
        if live_object.digest() != provided_ref.2 {
            return Err(
                anyhow::Error::new(SuiError::from(UserInputError::InvalidObjectDigest {
                    object_id: provided_ref.0,
                    expected_digest: live_object.digest(),
                }))
                .context(LIVE_STATE_LAG_NOTE),
            );
        }

        checkpoint_object_answers.push(live_object);
    }

    Ok(())
}

/// Build the exact protocol configuration named by the checkpoint.
pub fn protocol_config_for_snapshot(
    snapshot: &CheckpointExecutionContext,
) -> Result<ProtocolConfig> {
    let version = ProtocolVersion::new(snapshot.epoch.protocol_version);
    if version < ProtocolVersion::MIN || version > ProtocolVersion::MAX_ALLOWED {
        bail!(
            concat!(
                "protocol version {} at checkpoint {} is not supported by this binary ",
                "(supported: {} through {})",
            ),
            version.as_u64(),
            snapshot.checkpoint,
            ProtocolVersion::MIN.as_u64(),
            ProtocolVersion::MAX_ALLOWED.as_u64(),
        );
    }

    Ok(ProtocolConfig::get_for_version(
        version,
        snapshot.chain_identifier.chain(),
    ))
}

/// Reject valid transaction features whose required state or execution semantics are unavailable
/// to the initial local simulator.
fn reject_unsupported_shapes(
    prepared: &super::PreparedLocalSimulationTransaction,
    protocol_config: &ProtocolConfig,
) -> Result<()> {
    let transaction = &prepared.transaction;
    if !prepared.receiving_object_refs.is_empty() {
        return Err(unsupported_error(
            "local simulation does not support Receiving object inputs",
        ));
    }
    if protocol_config.enable_gasless() && transaction.is_gasless_transaction() {
        return Err(unsupported_error(
            "local simulation does not support gasless transactions",
        ));
    }
    if transaction.kind().get_funds_withdrawals().next().is_some() {
        return Err(unsupported_error(
            "local simulation does not support FundsWithdrawal inputs",
        ));
    }
    if transaction
        .kind()
        .get_coin_reservation_obj_refs()
        .next()
        .is_some()
    {
        return Err(unsupported_error(
            "local simulation does not support coin reservations in PTB inputs",
        ));
    }
    if transaction
        .gas()
        .iter()
        .any(|object_ref| ParsedDigest::is_coin_reservation_digest(&object_ref.2))
    {
        return Err(unsupported_error(
            "local simulation does not support coin reservations in gas payment",
        ));
    }
    if transaction.is_gas_paid_from_address_balance() {
        return Err(unsupported_error(
            "local simulation does not support gas payment from address balance",
        ));
    }

    Ok(())
}

/// Translate input object kind into a checkpoint object request.
fn checkpoint_request(kind: &InputObjectKind) -> CheckpointObjectRequest {
    let selector = match kind {
        InputObjectKind::MovePackage(_) => CheckpointObjectSelector::Latest,
        InputObjectKind::ImmOrOwnedMoveObject((_, version, _)) => {
            CheckpointObjectSelector::ExactVersion(version.value())
        }
        InputObjectKind::SharedMoveObject { .. } => CheckpointObjectSelector::Latest,
    };
    CheckpointObjectRequest {
        object_id: kind.object_id(),
        selector,
    }
}

/// Reject shared state whose checkpoint identity cannot represent the submitted input.
fn validate_loaded_input(
    kind: &InputObjectKind,
    object: &Object,
    snapshot: &CheckpointExecutionContext,
) -> Result<()> {
    if let InputObjectKind::SharedMoveObject { id, .. } = kind
        && object.full_id() != kind.full_object_id()
    {
        return Err(unsupported_error(format!(
            concat!(
                "local simulation cannot represent shared input {} because its live consensus ",
                "identity differs at checkpoint {}",
            ),
            id, snapshot.checkpoint,
        )));
    }

    Ok(())
}

/// Convert a missing checkpoint result into the most accurate error supported by current APIs.
///
/// Missing shared state is ambiguous because the checkpoint API cannot distinguish a nonexistent
/// object from a consensus stream that ended, so it is reported as unsupported.
fn missing_object_error(
    kind: &InputObjectKind,
    snapshot: &CheckpointExecutionContext,
) -> anyhow::Error {
    if let InputObjectKind::SharedMoveObject { id, .. } = kind {
        return unsupported_error(format!(
            concat!(
                "local simulation cannot distinguish missing shared input {} from an ended ",
                "consensus stream at checkpoint {}",
            ),
            id, snapshot.checkpoint,
        ));
    }

    anyhow::Error::new(SuiError::from(kind.object_not_found_error()))
}

/// Wrap a local-simulator capability boundary in Sui's standard unsupported-feature error.
pub(super) fn unsupported_error(error: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(SuiError::from(SuiErrorKind::UnsupportedFeatureError {
        error: error.into(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_data_store::EpochData;
    use sui_types::{
        base_types::{ObjectRef, SequenceNumber, SuiAddress},
        coin_reservation::ParsedObjectRefWithdrawal,
        digests::{ChainIdentifier, CheckpointDigest, ObjectDigest},
        gas_coin::GAS,
        object::{Object, Owner},
        transaction::{
            CallArg, FundsWithdrawalArg, ObjectArg, ProgrammableTransaction, SharedObjectMutability,
        },
    };

    struct TestStore {
        context: CheckpointExecutionContext,
        objects: Vec<(CheckpointObjectRequest, Object)>,
    }

    impl CheckpointObjectStore for TestStore {
        fn get_checkpoint_objects(
            &self,
            context: &CheckpointExecutionContext,
            requests: &[CheckpointObjectRequest],
        ) -> Result<Vec<Option<Object>>> {
            assert_eq!(context, &self.context);
            Ok(requests
                .iter()
                .map(|request| {
                    self.objects
                        .iter()
                        .find(|(candidate, _)| candidate == request)
                        .map(|(_, object)| object.clone())
                })
                .collect())
        }
    }

    fn context() -> CheckpointExecutionContext {
        CheckpointExecutionContext {
            chain_identifier: ChainIdentifier::default(),
            checkpoint: 17,
            checkpoint_digest: CheckpointDigest::random(),
            epoch: EpochData {
                epoch_id: 3,
                protocol_version: ProtocolConfig::get_for_max_version_UNSAFE()
                    .version
                    .as_u64(),
                rgp: 1_000,
                start_timestamp: 123,
            },
        }
    }

    fn transaction(
        sender: SuiAddress,
        inputs: Vec<CallArg>,
        payment: Vec<ObjectRef>,
    ) -> TransactionData {
        TransactionData::new_programmable(
            sender,
            payment,
            ProgrammableTransaction {
                inputs,
                commands: vec![],
            },
            ProtocolConfig::get_for_max_version_UNSAFE().max_tx_gas(),
            1_000,
        )
    }

    fn request(object_id: ObjectID, selector: CheckpointObjectSelector) -> CheckpointObjectRequest {
        CheckpointObjectRequest {
            object_id,
            selector,
        }
    }

    fn owned_object(object_id: ObjectID, version: u64, owner: SuiAddress) -> Object {
        Object::with_id_owner_version_for_testing(
            object_id,
            SequenceNumber::from_u64(version),
            Owner::AddressOwner(owner),
        )
    }

    fn unsupported_shape_error(transaction: TransactionData, receiving: Vec<ObjectRef>) -> String {
        let prepared = super::super::PreparedLocalSimulationTransaction {
            transaction,
            input_object_kinds: vec![],
            receiving_object_refs: receiving,
            mock_gas_object: None,
        };
        reject_unsupported_shapes(&prepared, &ProtocolConfig::get_for_max_version_UNSAFE())
            .unwrap_err()
            .to_string()
    }

    /// The transaction declares an owned input at version 5 and that body still exists at the
    /// checkpoint, but the object has since advanced to version 6. Preparation must reject the
    /// stale reference with the same error a fullnode reports for an already-consumed object.
    #[test]
    fn owned_input_version_mismatch_is_rejected() {
        let sender = SuiAddress::random_for_testing_only();
        let object_id = ObjectID::random();
        let declared = owned_object(object_id, 5, sender);
        let store = TestStore {
            context: context(),
            objects: vec![
                (
                    request(object_id, CheckpointObjectSelector::ExactVersion(5)),
                    declared.clone(),
                ),
                (
                    request(object_id, CheckpointObjectSelector::Latest),
                    owned_object(object_id, 6, sender),
                ),
            ],
        };
        let transaction = transaction(
            sender,
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(
                declared.compute_object_reference(),
            ))],
            vec![],
        );

        let error = prepare_checked_local_dry_run(transaction, store.context.clone(), &store, true)
            .err()
            .unwrap();

        assert!(matches!(
            error.downcast_ref::<SuiError>().unwrap().as_inner(),
            SuiErrorKind::UserInputError {
                error: UserInputError::ObjectVersionUnavailableForConsumption {
                    current_version,
                    ..
                },
            } if current_version.value() == 6
        ));
    }

    /// The live object matches the declared version, but its body — and therefore its digest —
    /// differs from the declared reference, meaning the source returned inconsistent state.
    /// Preparation must reject it rather than execute a body the submitter never named.
    #[test]
    fn owned_input_digest_mismatch_is_rejected() {
        let sender = SuiAddress::random_for_testing_only();
        let object_id = ObjectID::random();
        let declared = owned_object(object_id, 5, sender);
        let store = TestStore {
            context: context(),
            objects: vec![
                (
                    request(object_id, CheckpointObjectSelector::ExactVersion(5)),
                    declared.clone(),
                ),
                (
                    request(object_id, CheckpointObjectSelector::Latest),
                    owned_object(object_id, 5, SuiAddress::random_for_testing_only()),
                ),
            ],
        };
        let transaction = transaction(
            sender,
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(
                declared.compute_object_reference(),
            ))],
            vec![],
        );

        let error = prepare_checked_local_dry_run(transaction, store.context.clone(), &store, true)
            .err()
            .unwrap();

        assert!(matches!(
            error.downcast_ref::<SuiError>().unwrap().as_inner(),
            SuiErrorKind::UserInputError {
                error: UserInputError::InvalidObjectDigest { object_id: mismatched, .. },
            } if *mismatched == object_id
        ));
    }

    /// The transaction names a shared input first shared at version 5, but the live object's
    /// consensus identity says it became shared at version 6 — a different incarnation of the
    /// same ID. Checkpoint state cannot represent the submitted input, so preparation reports
    /// it as unsupported rather than executing against the wrong incarnation.
    #[test]
    fn shared_input_identity_mismatch_is_unsupported() {
        let object_id = ObjectID::random();
        let live = Object::with_id_owner_version_for_testing(
            object_id,
            SequenceNumber::from_u64(6),
            Owner::Shared {
                initial_shared_version: SequenceNumber::from_u64(6),
            },
        );
        let store = TestStore {
            context: context(),
            objects: vec![(request(object_id, CheckpointObjectSelector::Latest), live)],
        };
        let transaction = transaction(
            SuiAddress::random_for_testing_only(),
            vec![CallArg::Object(ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: SequenceNumber::from_u64(5),
                mutability: SharedObjectMutability::Mutable,
            })],
            vec![],
        );

        let error = prepare_checked_local_dry_run(transaction, store.context.clone(), &store, true)
            .err()
            .unwrap();

        assert!(matches!(
            error.downcast_ref::<SuiError>().unwrap().as_inner(),
            SuiErrorKind::UnsupportedFeatureError { .. }
        ));
        assert!(error.to_string().contains("consensus identity differs"));
    }

    /// Owned inputs are fetched at their declared version; packages and shared inputs carry no
    /// usable declared version and follow the latest state at the checkpoint.
    #[test]
    fn checkpoint_requests_map_selectors_by_input_kind() {
        let object_id = ObjectID::random();
        let owned = InputObjectKind::ImmOrOwnedMoveObject((
            object_id,
            SequenceNumber::from_u64(5),
            ObjectDigest::random(),
        ));
        let package = InputObjectKind::MovePackage(object_id);
        let shared = InputObjectKind::SharedMoveObject {
            id: object_id,
            initial_shared_version: SequenceNumber::from_u64(1),
            mutability: SharedObjectMutability::Mutable,
        };

        assert_eq!(
            checkpoint_request(&owned),
            request(object_id, CheckpointObjectSelector::ExactVersion(5)),
        );
        assert_eq!(
            checkpoint_request(&package),
            request(object_id, CheckpointObjectSelector::Latest),
        );
        assert_eq!(
            checkpoint_request(&shared),
            request(object_id, CheckpointObjectSelector::Latest),
        );
    }

    /// Receiving inputs, funds withdrawals, coin reservations, and address-balance gas payment
    /// are valid transaction features whose required state or semantics are unavailable to the
    /// local simulator, so each is rejected up front with its specific unsupported-feature error.
    #[test]
    fn unsupported_shapes_are_rejected() {
        let sender = SuiAddress::random_for_testing_only();
        let gas_ref = (
            ObjectID::random(),
            SequenceNumber::from_u64(1),
            ObjectDigest::random(),
        );
        let receiving_ref = (
            ObjectID::random(),
            SequenceNumber::from_u64(1),
            ObjectDigest::random(),
        );
        let reservation_ref = ParsedObjectRefWithdrawal::new(ObjectID::random(), 3, 100)
            .encode(SequenceNumber::from_u64(2), ChainIdentifier::default());

        assert!(
            unsupported_shape_error(
                transaction(sender, vec![], vec![gas_ref]),
                vec![receiving_ref]
            )
            .contains("Receiving object inputs")
        );
        assert!(
            unsupported_shape_error(
                transaction(
                    sender,
                    vec![CallArg::FundsWithdrawal(
                        FundsWithdrawalArg::balance_from_sender(1, GAS::type_tag()),
                    )],
                    vec![gas_ref],
                ),
                vec![],
            )
            .contains("FundsWithdrawal inputs")
        );
        assert!(
            unsupported_shape_error(
                transaction(
                    sender,
                    vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(
                        reservation_ref
                    ))],
                    vec![gas_ref],
                ),
                vec![],
            )
            .contains("coin reservations in PTB inputs")
        );
        assert!(
            unsupported_shape_error(transaction(sender, vec![], vec![reservation_ref]), vec![])
                .contains("coin reservations in gas payment")
        );
        assert!(
            unsupported_shape_error(transaction(sender, vec![], vec![]), vec![])
                .contains("gas payment from address balance")
        );
    }
}
