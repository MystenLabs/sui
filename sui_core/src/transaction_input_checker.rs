// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use sui_types::{
    base_types::{ObjectRef, SequenceNumber, SuiAddress},
    error::{SuiError, SuiResult},
    fp_ensure, gas,
    messages::{InputObjectKind, SingleTransactionKind, Transaction},
    object::{Object, Owner},
};
use tracing::debug;

/// Check all the objects used in the transaction against the database, and ensure
/// that they are all the correct version and number.
pub fn check_locks(
    transaction: &Transaction,
    input_objects: Vec<InputObjectKind>,
    objects: Vec<Option<Object>>,
) -> Result<Vec<(InputObjectKind, Object)>, SuiError> {
    // Constructing the list of objects that could be used to authenticate other
    // objects. Any mutable object (either shared or owned) can be used to
    // authenticate other objects. Hence essentially we are building the list
    // of mutable objects.
    // We require that mutable objects cannot show up more than once.
    // In [`SingleTransactionKind::input_objects`] we checked that there is no
    // duplicate objects in the same SingleTransactionKind. However for a Batch
    // Transaction, we still need to make sure that the same mutable object don't show
    // up in more than one SingleTransactionKind.
    // TODO: We should be able to allow the same shared mutable object to show up
    // in more than one SingleTransactionKind. We need to ensure that their
    // version number only increases once at the end of the Batch execution.
    let mut owned_object_authenticators: HashSet<SuiAddress> = HashSet::new();
    for object in objects.iter().flatten() {
        if !object.is_read_only() {
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
    for (object_kind, object) in input_objects.into_iter().zip(objects) {
        // All objects must exist in the DB.
        let object = match object {
            Some(object) => object,
            None => {
                errors.push(object_kind.object_not_found_error());
                continue;
            }
        };
        // Check if the object contents match the type of lock we need for
        // this object.
        match check_one_lock(
            transaction,
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
    check_tx_requirement(transaction, &all_objects)?;
    Ok(all_objects)
}

pub fn filter_owned_objects(all_objects: Vec<(InputObjectKind, Object)>) -> Vec<ObjectRef> {
    let owned_objects: Vec<_> = all_objects
        .into_iter()
        .filter_map(|(object_kind, object)| match object_kind {
            InputObjectKind::MovePackage(_) => None,
            InputObjectKind::ImmOrOwnedMoveObject(object_ref) => {
                if object.is_read_only() {
                    None
                } else {
                    Some(object_ref)
                }
            }
            InputObjectKind::MutSharedMoveObject(..) => None,
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
    transaction: &Transaction,
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
            fp_ensure!(
                object.digest() == object_digest,
                SuiError::InvalidObjectDigest {
                    object_id,
                    expected_digest: object_digest
                }
            );

            match object.owner {
                Owner::SharedImmutable => {
                    // Nothing else to check for SharedImmutable.
                }
                Owner::AddressOwner(owner) => {
                    // Check the owner is the transaction sender.
                    fp_ensure!(
                        transaction.sender_address() == owner,
                        SuiError::IncorrectSigner {
                            error: format!("Object {:?} is owned by account address {:?}, but signer address is {:?}", object_id, owner, transaction.sender_address()),
                        }
                    );
                }
                Owner::ObjectOwner(owner) => {
                    // Check that the object owner is another mutable object in the input.
                    fp_ensure!(
                        owned_object_authenticators.contains(&owner),
                        SuiError::IncorrectSigner {
                            error: format!(
                                "Object {:?} is owned by object {:?}, which is not in the input",
                                object.id(),
                                owner
                            ),
                        }
                    );
                }
                Owner::SharedMutable => {
                    // This object is a mutable shared object. However the transaction
                    // specifies it as an owned object. This is inconsistent.
                    return Err(SuiError::NotSharedObjectError);
                }
            };
        }
        InputObjectKind::MutSharedMoveObject(..) => {
            // When someone locks an object as shared it must be shared already.
            fp_ensure!(object.is_shared(), SuiError::NotSharedObjectError);
        }
    };
    Ok(())
}

/// This function does 3 things:
/// 1. Check if the gas object has enough balance to pay for this transaction.
///   Since the transaction may be a batch transaction, we need to walk through
///   each single transaction in it and accumulate their gas cost. For Move call
///   and publish we can simply use their budget, for transfer we will calculate
///   the cost on the spot since it's deterministic (See comments inside the function).
/// 2. Check if the gas budget for each single transction is above some minimum amount.
///   This can help reduce DDos attacks.
/// 3. Check that the objects used in transfers are mutable. We put the check here
///   because this is the most convenient spot to check.
fn check_tx_requirement(
    transaction: &Transaction,
    input_objects: &[(InputObjectKind, Object)],
) -> SuiResult {
    let mut total_cost = 0;
    let mut idx = 0;
    for tx in transaction.single_transactions() {
        match tx {
            SingleTransactionKind::Transfer(_) => {
                // Index access safe because the inputs were constructed in order.
                let transfer_object = &input_objects[idx].1;
                transfer_object.is_transfer_eligible()?;
                // TODO: Make Transfer transaction to also contain gas_budget.
                // By @gdanezis: Now his is the only part of this function that requires
                // an input object besides the gas object. It would be a major win if we
                // can get rid of the requirement to have all objects to check the transfer
                // requirement. If we can go this, then we could execute this check before
                // we check for signatures.
                // This would allow us to shore up out DoS defences: we only need to do a
                // read on the gas object balance before we do anything expensive,
                // such as checking signatures.
                total_cost += gas::calculate_object_transfer_cost(transfer_object);
                idx += tx.input_object_count();
            }
            SingleTransactionKind::Call(op) => {
                gas::check_move_gas_requirement(op.gas_budget)?;
                total_cost += op.gas_budget;
                idx += tx.input_object_count();
            }
            SingleTransactionKind::Publish(op) => {
                gas::check_move_gas_requirement(op.gas_budget)?;
                total_cost += op.gas_budget;
                // No need to update idx because Publish cannot show up in batch.
            }
        }
    }
    // The last element in the inputs is always gas object.
    let gas_object = &input_objects.last().unwrap().1;
    fp_ensure!(
        !gas_object.is_shared(),
        SuiError::InsufficientGas {
            error: format!("Gas object cannot be shared: {:?}", gas_object.id())
        }
    );
    gas::check_gas_balance(gas_object, total_cost)
}
