// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use prometheus_exporter::prometheus::IntCounter;
use serde::{Deserialize, Serialize};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    error::{SuiError, SuiResult},
    fp_ensure,
    gas::{self, SuiGasStatus},
    messages::{InputObjectKind, SingleTransactionKind, TransactionData, TransactionEnvelope},
    object::{Object, Owner},
};
use tracing::{debug, instrument};

use crate::authority::SuiDataStore;

#[instrument(level = "trace", skip_all)]
pub async fn check_transaction_input<const A: bool, const B: bool, S, T>(
    store: &SuiDataStore<A, B, S>,
    transaction: &TransactionEnvelope<T>,
    shared_obj_metric: &IntCounter,
) -> Result<(SuiGasStatus<'static>, Vec<(InputObjectKind, Object)>), SuiError>
where
    S: Eq + Serialize + for<'de> Deserialize<'de>,
{
    let mut gas_status = check_gas(
        store,
        transaction.gas_payment_object_ref().0,
        transaction.data.gas_budget,
        transaction.data.kind.is_system_tx(),
    )
    .await?;

    let objects_by_kind = check_locks(store, &transaction.data).await?;

    if transaction.contains_shared_object() {
        shared_obj_metric.inc();

        // It's important that we do this here to make sure there is enough
        // gas to cover shared objects, before we lock all objects.
        gas_status.charge_consensus()?;
    }

    Ok((gas_status, objects_by_kind))
}

/// Checking gas budget by fetching the gas object only from the store,
/// and check whether the balance and budget satisfies the miminum requirement.
/// Returns the gas object (to be able to reuse it latter) and a gas status
/// that will be used in the entire lifecycle of the transaction execution.
#[instrument(level = "trace", skip_all)]
async fn check_gas<const A: bool, const B: bool, S>(
    store: &SuiDataStore<A, B, S>,
    gas_payment_id: ObjectID,
    gas_budget: u64,
    is_system_tx: bool,
) -> SuiResult<SuiGasStatus<'static>>
where
    S: Eq + Serialize + for<'de> Deserialize<'de>,
{
    if is_system_tx {
        Ok(SuiGasStatus::new_unmetered())
    } else {
        let gas_object = store.get_object(&gas_payment_id)?;
        let gas_object = gas_object.ok_or(SuiError::ObjectNotFound {
            object_id: gas_payment_id,
        })?;
        gas::check_gas_balance(&gas_object, gas_budget)?;
        // TODO: Pass in real computation gas unit price and storage gas unit price.
        let gas_status = gas::start_gas_metering(gas_budget, 1, 1)?;
        Ok(gas_status)
    }
}

#[instrument(level = "trace", skip_all, fields(num_objects = input_objects.len()))]
async fn fetch_objects<const A: bool, const B: bool, S>(
    store: &SuiDataStore<A, B, S>,
    input_objects: &[InputObjectKind],
) -> Result<Vec<Option<Object>>, SuiError>
where
    S: Eq + Serialize + for<'de> Deserialize<'de>,
{
    let ids: Vec<_> = input_objects.iter().map(|kind| kind.object_id()).collect();
    store.get_objects(&ids[..])
}

/// Check all the objects used in the transaction against the database, and ensure
/// that they are all the correct version and number.
#[instrument(level = "trace", skip_all)]
async fn check_locks<const A: bool, const B: bool, S>(
    store: &SuiDataStore<A, B, S>,
    transaction: &TransactionData,
) -> Result<Vec<(InputObjectKind, Object)>, SuiError>
where
    S: Eq + Serialize + for<'de> Deserialize<'de>,
{
    let input_objects = transaction.input_objects()?;
    // These IDs act as authenticators that can own other objects.
    let objects = fetch_objects(store, &input_objects).await?;

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
    let mut owned_object_authenticators: HashSet<SuiAddress> = HashSet::new();
    for object in objects.iter().flatten() {
        if !object.is_immutable() {
            fp_ensure!(
                owned_object_authenticators.insert(object.id().into()),
                SuiError::InvalidBatchTransaction {
                    error: format!("Mutable object {} cannot appear in more than one single transactions in a batch", object.id()),
                }
            );
        }
    }

    // Gather all objects and errors.
    let mut all_objects = Vec::with_capacity(input_objects.len());
    let mut errors = Vec::new();
    let transfer_object_ids: HashSet<_> = transaction
        .kind
        .single_transactions()
        .filter_map(|s| {
            if let SingleTransactionKind::TransferCoin(t) = s {
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
            object.is_transfer_eligible()?;
        }
        // Check if the object contents match the type of lock we need for
        // this object.
        match check_one_lock(
            &transaction.signer(),
            object_kind,
            &object,
            &owned_object_authenticators,
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
        return Err(SuiError::LockErrors { errors });
    }
    fp_ensure!(!all_objects.is_empty(), SuiError::ObjectInputArityViolation);

    Ok(all_objects)
}

pub fn filter_owned_objects(all_objects: &[(InputObjectKind, Object)]) -> Vec<ObjectRef> {
    let owned_objects: Vec<_> = all_objects
        .iter()
        .filter_map(|(object_kind, object)| match object_kind {
            InputObjectKind::MovePackage(_) => None,
            InputObjectKind::ImmOrOwnedMoveObject(object_ref) => {
                if object.is_immutable() {
                    None
                } else {
                    Some(*object_ref)
                }
            }
            InputObjectKind::SharedMoveObject(_) => None,
        })
        .collect();

    debug!(
        num_mutable_objects = owned_objects.len(),
        "Checked locks and found mutable objects"
    );

    owned_objects
}

/// The logic to check one object against a reference, and return the object if all is well
/// or an error if not.
fn check_one_lock(
    sender: &SuiAddress,
    object_kind: InputObjectKind,
    object: &Object,
    owned_object_authenticators: &HashSet<SuiAddress>,
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
            fp_ensure!(
                sequence_number <= SequenceNumber::MAX,
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
                Owner::ObjectOwner(owner) => {
                    // Check that the object owner is another mutable object in the input.
                    fp_ensure!(
                        owned_object_authenticators.contains(&owner),
                        SuiError::MissingObjectOwner {
                            child_id: object.id(),
                            parent_id: owner.into(),
                        }
                    );
                }
                Owner::Shared => {
                    // This object is a mutable shared object. However the transaction
                    // specifies it as an owned object. This is inconsistent.
                    return Err(SuiError::NotSharedObjectError);
                }
            };
        }
        InputObjectKind::SharedMoveObject(..) => {
            // When someone locks an object as shared it must be shared already.
            fp_ensure!(object.is_shared(), SuiError::NotSharedObjectError);
        }
    };
    Ok(())
}
