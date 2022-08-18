// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::SuiDataStore;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Debug;
use sui_adapter::object_root_ancestor_map::ObjectRootAncestorMap;
use sui_types::messages::TransactionKind;
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
pub async fn check_transaction_input<S, T>(
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

    let input_objects = check_objects(store, &transaction.signed_data.data).await?;

    if transaction.contains_shared_object() {
        // It's important that we do this here to make sure there is enough
        // gas to cover shared objects, before we lock all objects.
        gas_status.charge_consensus()?;
    }

    Ok((gas_status, input_objects))
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

/// Check all the objects used in the transaction against the database, and ensure
/// that they are all the correct version and number.
#[instrument(level = "trace", skip_all)]
async fn check_objects<S>(
    store: &SuiDataStore<S>,
    transaction: &TransactionData,
) -> Result<InputObjects, SuiError>
where
    S: Eq + Debug + Serialize + for<'de> Deserialize<'de>,
{
    let input_objects = transaction.input_objects()?;

    // These IDs act as authenticators that can own other objects.
    let objects = store.get_input_objects(&input_objects)?;

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
    for object in objects.iter().flatten() {
        if !object.is_immutable() {
            fp_ensure!(
                object_owner_map.insert(object.id(), object.owner).is_none(),
                SuiError::InvalidBatchTransaction {
                    error: format!("Mutable object {} cannot appear in more than one single transactions in a batch", object.id()),
                }
            );
        }
    }

    // build a map of each object to its ancestor object (if it has one)
    // returns an error on cycles, so this should not occur as storage should be free of cycles
    let root_ancestor_map =
        ObjectRootAncestorMap::new(&object_owner_map).expect("ownership should be well formed");
    // Gather all objects and errors.
    let mut all_objects = Vec::with_capacity(input_objects.len());
    let mut errors = Vec::new();
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

    for (object_kind, object) in input_objects.into_iter().zip(objects) {
        // All objects must exist in the DB.
        let object = match object {
            Some(object) => object,
            None => {
                errors.push(object_kind.object_not_found_error());
                continue;
            }
        };
        if transfer_object_ids.contains(&object.id()) {
            object.ensure_public_transfer_eligible()?;
        }
        // Check if the object contents match the type of lock we need for
        // this object.
        match check_one_object(
            &transaction.signer(),
            object_kind,
            &object,
            &root_ancestor_map,
        ) {
            Ok(()) => all_objects.push((object_kind, object)),
            Err(e) => {
                errors.push(e);
            }
        }
    }
    // If any errors with the locks were detected, we return all errors to give the client
    // a chance to update the authority if possible.
    if !errors.is_empty() {
        return Err(SuiError::ObjectErrors { errors });
    }
    fp_ensure!(!all_objects.is_empty(), SuiError::ObjectInputArityViolation);

    Ok(InputObjects::new(all_objects))
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
            // Note that this generally can't fail, because we fetch objects at the version
            // specified by the input objects. This makes check_transaction_input idempotent.
            // A tx that tries to operate on older versions will fail later when checking the
            // object locks.
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
