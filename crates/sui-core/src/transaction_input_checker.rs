// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::SuiDataStore;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Debug;
use sui_adapter::object_root_ancestor_map::ObjectRootAncestorMap;
use sui_types::messages::TransactionKind;
use sui_types::object::Data;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    error::{SuiError, SuiResult},
    fp_ensure,
    gas::{self, SuiGasStatus},
    messages::{
        InputObjectKind, InputObjects, SingleTransactionKind, TransactionData, TransactionEnvelope,
    },
    object::{Object, Owner},
};
use tracing::instrument;

#[instrument(level = "trace", skip_all)]
pub async fn check_transaction_input_for_certificate_generation<S, T>(
    store: &SuiDataStore<S>,
    transaction: &TransactionEnvelope<T>,
) -> Result<(SuiGasStatus<'static>, InputObjects), SuiError>
where
    S: Eq + Debug + Serialize + for<'de> Deserialize<'de>,
{
    let mut gas_status = check_gas(
        store,
        transaction.gas_payment_object_ref().0,
        transaction.signed_data.data.gas_budget,
        transaction.signed_data.data.gas_price,
        &transaction.signed_data.data.kind,
    )
    .await?;

    let input_object_kinds = transaction.signed_data.data.input_objects()?;
    // These IDs act as authenticators that can own other objects.
    let fetched_input_objects = fetch_objects(store, &input_object_kinds).await?;
    let (input_objects, errors) = check_objects(
        &transaction.signed_data.data,
        input_object_kinds,
        fetched_input_objects,
    );
    if !errors.is_empty() {
        let errors = errors.into_iter().map(|(_, _, e)| e).collect();
        return Err(SuiError::ObjectErrors { errors });
    }
    fp_ensure!(
        !input_objects.is_empty(),
        SuiError::ObjectInputArityViolation
    );
    if transaction.contains_shared_object() {
        // It's important that we do this here to make sure there is enough
        // gas to cover shared objects, before we lock all objects.
        gas_status.charge_consensus()?;
    }

    Ok((gas_status, input_objects))
}

pub async fn check_transaction_input_for_certificate_execution<S, T>(
    store: &SuiDataStore<S>,
    transaction: &TransactionEnvelope<T>,
) -> Result<
    (
        SuiGasStatus<'static>,
        InputObjects,
        Vec<(ObjectID, SuiError)>,
    ),
    SuiError,
>
where
    S: Eq + Debug + Serialize + for<'de> Deserialize<'de>,
{
    macro_rules! unexpected_error {
        () => {
            "Should have been verified during certificate generation"
        };
    }
    let gas_status_res = check_gas(
        store,
        transaction.gas_payment_object_ref().0,
        transaction.signed_data.data.gas_budget,
        transaction.signed_data.data.gas_price,
        &transaction.signed_data.data.kind,
    )
    .await;
    debug_assert!(gas_status_res.is_ok(), unexpected_error!());
    let mut gas_status = gas_status_res?;

    let input_object_kinds_res = transaction.signed_data.data.input_objects();
    debug_assert!(input_object_kinds_res.is_ok(), unexpected_error!());
    let input_object_kinds = input_object_kinds_res?;

    // These IDs act as authenticators that can own other objects.
    let fetched_input_objects_res = fetch_objects(store, &input_object_kinds).await;
    debug_assert!(fetched_input_objects_res.is_ok(), unexpected_error!());
    let fetched_input_objects = fetched_input_objects_res?;
    let (input_objects, errors) = check_objects(
        &transaction.signed_data.data,
        input_object_kinds,
        fetched_input_objects,
    ); // errors is empty implies all_objects is not empty
    if errors.is_empty() && input_objects.is_empty() {
        debug_assert!(false, unexpected_error!());
        return Err(SuiError::ObjectInputArityViolation);
    }
    let mut shared_or_quasi_shared_errors = vec![];
    let mut non_shared_errors = vec![];
    for (err_obj_id, is_shared_or_quasi_shared, e) in errors {
        if is_shared_or_quasi_shared {
            debug_assert!(
                matches!(
                    e,
                    SuiError::ObjectNotFound { .. }
                        | SuiError::NotQuasiSharedObject { .. }
                        | SuiError::InvalidSequenceNumber { .. }
                ),
                unexpected_error!()
            );
            shared_or_quasi_shared_errors.push((err_obj_id, e))
        } else {
            debug_assert!(false, unexpected_error!());
            non_shared_errors.push(e)
        }
    }
    if !non_shared_errors.is_empty() {
        return Err(SuiError::ObjectErrors {
            errors: non_shared_errors,
        });
    }

    if transaction.contains_shared_object() {
        // It's important that we do this here to make sure there is enough
        // gas to cover shared objects, before we lock all objects.
        let charge_res = gas_status.charge_consensus();
        debug_assert!(charge_res.is_ok(), unexpected_error!());
        charge_res?
    }

    Ok((gas_status, input_objects, shared_or_quasi_shared_errors))
}

/// Checking gas budget by fetching the gas object only from the store,
/// and check whether the balance and budget satisfies the miminum requirement.
/// Returns the gas object (to be able to reuse it latter) and a gas status
/// that will be used in the entire lifecycle of the transaction execution.
#[instrument(level = "trace", skip_all)]
async fn check_gas<S>(
    store: &SuiDataStore<S>,
    gas_payment_id: ObjectID,
    gas_budget: u64,
    computation_gas_price: u64,
    tx_kind: &TransactionKind,
) -> SuiResult<SuiGasStatus<'static>>
where
    S: Eq + Debug + Serialize + for<'de> Deserialize<'de>,
{
    if tx_kind.is_system_tx() {
        Ok(SuiGasStatus::new_unmetered())
    } else {
        let gas_object = store.get_object(&gas_payment_id)?;
        let gas_object = gas_object.ok_or(SuiError::ObjectNotFound {
            object_id: gas_payment_id,
        })?;

        //TODO: cache this storage_gas_price in memory
        let storage_gas_price = store
            .get_sui_system_state_object()?
            .parameters
            .storage_gas_price;

        // If the transaction is TransferSui, we ensure that the gas balance is enough to cover
        // both gas budget and the transfer amount.
        let extra_amount =
            if let TransactionKind::Single(SingleTransactionKind::TransferSui(t)) = tx_kind {
                t.amount.unwrap_or_default()
            } else {
                0
            };
        // TODO: We should revisit how we compute gas price and compare to gas budget.
        let gas_price = std::cmp::max(computation_gas_price, storage_gas_price);

        gas::check_gas_balance(&gas_object, gas_budget, gas_price, extra_amount)?;
        let gas_status =
            gas::start_gas_metering(gas_budget, computation_gas_price, storage_gas_price)?;
        Ok(gas_status)
    }
}

#[instrument(level = "trace", skip_all, fields(num_objects = input_objects.len()))]
async fn fetch_objects<S>(
    store: &SuiDataStore<S>,
    input_objects: &[InputObjectKind],
) -> Result<Vec<Option<Object>>, SuiError>
where
    S: Eq + Debug + Serialize + for<'de> Deserialize<'de>,
{
    let ids: Vec<_> = input_objects.iter().map(|kind| kind.object_id()).collect();
    store.get_objects(&ids[..])
}

/// Check all the objects used in the transaction against the database, and ensure
/// that they are all the correct version and number.
/// When processing the certificate we want to know if the error was associated with a shared or
/// quasi shared objects. The only errors that should be possible for those cases are:
/// - The (quasi-)shared object was deleted
/// - The quasi-shared object is no longer quasi-shared
///   (transferred to an owned object, made owned, frozen, etc)
#[instrument(level = "trace", skip_all)]
fn check_objects(
    transaction: &TransactionData,
    input_object_kinds: Vec<InputObjectKind>,
    fetched_input_objects: Vec<Option<Object>>,
) -> (
    InputObjects,
    Vec<(
        ObjectID,
        /* for shared or quasi-shared */ bool,
        SuiError,
    )>,
) {
    assert!(input_object_kinds.len() == fetched_input_objects.len());
    let mut errors: Vec<(ObjectID, bool, SuiError)> = vec![];
    // Constructing the list of objects that could be used to authenticate other
    // objects. Any mutable object (either shared or owned) can be used to
    // authenticate other objects. Hence essentially we are building the list
    // of mutable objects.
    // We require that mutable objects cannot show up more than once.
    // In [`SingleTransactionKind::input_objects`] we checked that there is no
    // duplicate objects in the same SingleTransactionKind. However for a Batch
    // Transaction, we still need to make sure that the same mutable object don't show
    // up in more than one SingleTransactionKind.
    // TODO: We should be able to allow the same shared object to show up
    // in more than one SingleTransactionKind. We need to ensure that their
    // version number only increases once at the end of the Batch execution.
    let mut object_owner_map = BTreeMap::new();
    for object in fetched_input_objects.iter().flatten() {
        if !object.is_immutable() {
            let object_id = object.id();
            if object_owner_map.insert(object.id(), object.owner).is_some() {
                // we do not care about this error for shared objects as it should not be possible
                // when processing a certificate
                let error = format!(
                    "Mutable object {} cannot appear in more than one single \
                    transactions in a batch",
                    object.id()
                );
                errors.push((
                    object_id,
                    false,
                    SuiError::InvalidBatchTransaction { error },
                ));
            }
        }
    }

    // build a map of each object to its ancestor object (if it has one)
    // returns an error on cycles, so this should not occur as storage should be free of cycles
    let root_ancestor_map =
        ObjectRootAncestorMap::new(&object_owner_map).expect("ownership should be well formed");
    // Gather all objects and errors.
    let mut all_objects = Vec::with_capacity(input_object_kinds.len());
    let transfer_object_ids: HashSet<_> = transaction
        .kind
        .single_transactions()
        .filter_map(|s| {
            if let SingleTransactionKind::TransferObject(t) = s {
                Some(t.object_ref.0)
            } else {
                None
            }
        })
        .collect();

    for (object_kind, object) in input_object_kinds.into_iter().zip(fetched_input_objects) {
        let is_shared_or_quasi_shared = matches!(
            object_kind,
            InputObjectKind::SharedMoveObject(_) | InputObjectKind::QuasiSharedMoveObject(_)
        );
        // All objects must exist in the DB.
        let object = match object {
            Some(object) => object,
            None => {
                errors.push((
                    object_kind.object_id(),
                    is_shared_or_quasi_shared,
                    object_kind.object_not_found_error(),
                ));
                continue;
            }
        };
        // Check if the object contents match the type of lock we need for
        // this object.
        let object_id = object.id();
        let correct_object_kind = match check_one_object(
            &transaction.signer(),
            object_kind,
            &object,
            &root_ancestor_map,
        ) {
            Err(e) => {
                // if there was an error in checking the object, there could be something off with
                // the object kind.
                // For correctly bumping the versions of shared objects, we need to make sure we
                // have the correct object kind
                errors.push((object_id, is_shared_or_quasi_shared, e));
                compute_object_kind(&object, &root_ancestor_map)
            }
            Ok(()) => object_kind,
        };
        if transfer_object_ids.contains(&object_id) {
            if let Err(e) = object.ensure_public_transfer_eligible() {
                errors.push((object_id, is_shared_or_quasi_shared, e.into()));
            }
        }
        all_objects.push((correct_object_kind, object));
    }
    (InputObjects::new(all_objects), errors)
}

/// The logic to check one object against a reference, and return the object if all is well
/// or an error if not.
fn check_one_object(
    sender: &SuiAddress,
    object_kind: InputObjectKind,
    object: &Object,
    root_ancestor_map: &ObjectRootAncestorMap,
) -> SuiResult {
    match object_kind {
        InputObjectKind::MovePackage(package_id) => {
            fp_ensure!(
                object.data.try_as_package().is_some(),
                SuiError::MoveObjectAsPackage {
                    object_id: package_id
                }
            );
        }
        InputObjectKind::ImmOrOwnedMoveObject((object_id, sequence_number, object_digest)) => {
            fp_ensure!(
                !object.is_package(),
                SuiError::MovePackageAsObject { object_id }
            );
            // wrapped objects that are then deleted will be set to MAX,
            // so we need to cap the sequence number at MAX - 1
            fp_ensure!(
                sequence_number < SequenceNumber::MAX.decrement().unwrap(),
                SuiError::InvalidSequenceNumber
            );

            // Check that the seq number is the same
            fp_ensure!(
                object.version() == sequence_number,
                SuiError::UnexpectedSequenceNumber {
                    object_id,
                    expected_sequence: object.version(),
                    given_sequence: sequence_number,
                }
            );

            // Check the digest matches

            let expected_digest = object.digest();
            fp_ensure!(
                expected_digest == object_digest,
                SuiError::InvalidObjectDigest {
                    object_id,
                    expected_digest
                }
            );

            match object.owner {
                Owner::Immutable => {
                    // Nothing else to check for Immutable.
                }
                Owner::AddressOwner(owner) => {
                    // Check the owner is the transaction sender.
                    fp_ensure!(
                        sender == &owner,
                        SuiError::IncorrectSigner {
                            error: format!("Object {:?} is owned by account address {:?}, but signer address is {:?}", object_id, owner, sender),
                        }
                    );
                }
                Owner::ObjectOwner(_) => {
                    // object guaranteed to have an ancestor, as the map was built on the
                    //
                    let (_, ancestor_owner) = root_ancestor_map
                        .get_root_ancestor(&object_id)
                        .expect("ownership should be well formed");
                    fp_ensure!(
                        !ancestor_owner.is_shared(),
                        SuiError::NotImmutableOrOwnedObject {
                            object_id,
                            reason: "The root ancestor is a shared object".to_owned(),
                        }
                    );
                }
                Owner::Shared => {
                    // This object is a mutable shared object. However the transaction
                    // specifies it as an owned object. This is inconsistent.
                    let reason = "This object is a shared object".to_owned();
                    return Err(SuiError::NotImmutableOrOwnedObject { object_id, reason });
                }
            };
        }
        InputObjectKind::SharedMoveObject(_) => {
            fp_ensure!(
                object.version() < SequenceNumber::MAX,
                SuiError::InvalidSequenceNumber
            );
            // When someone locks an object as shared it must be shared already.
            fp_ensure!(object.is_shared(), SuiError::NotSharedObjectError);
        }
        InputObjectKind::QuasiSharedMoveObject(object_id) => {
            // wrapped objects that are then deleted will be set to MAX,
            // so we need to cap the sequence number at MAX - 1
            fp_ensure!(
                object.version() < SequenceNumber::MAX.decrement().unwrap(),
                SuiError::InvalidSequenceNumber
            );
            fp_ensure!(
                matches!(object.owner, Owner::ObjectOwner(_)),
                SuiError::NotQuasiSharedObject {
                    object_id,
                    reason: "The object is not owned by object".to_owned(),
                }
            );
            let (_, ancestor_owner) = root_ancestor_map
                .get_root_ancestor(&object.id())
                .expect("has an object owner, so it must have an ancestor");
            fp_ensure!(
                ancestor_owner.is_shared(),
                SuiError::NotQuasiSharedObject {
                    object_id,
                    reason: "The root ancestor object is not a shared object".to_owned(),
                }
            );
        }
    };
    Ok(())
}

fn compute_object_kind(
    object: &Object,
    root_ancestor_map: &ObjectRootAncestorMap,
) -> InputObjectKind {
    match (&object.data, &object.owner) {
        (Data::Package(_), _) => InputObjectKind::MovePackage(object.id()),
        (Data::Move(_), Owner::Shared) => InputObjectKind::SharedMoveObject(object.id()),
        (Data::Move(_), Owner::AddressOwner(_)) | (Data::Move(_), Owner::Immutable) => {
            InputObjectKind::ImmOrOwnedMoveObject(object.compute_object_reference())
        }
        (Data::Move(_), Owner::ObjectOwner(_)) => {
            let object_id = object.id();
            let (_, ancestor_owner) = root_ancestor_map
                .get_root_ancestor(&object_id)
                .expect("ownership should be well formed");
            if ancestor_owner.is_shared() {
                InputObjectKind::QuasiSharedMoveObject(object_id)
            } else {
                InputObjectKind::ImmOrOwnedMoveObject(object.compute_object_reference())
            }
        }
    }
}
