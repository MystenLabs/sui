// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::AuthorityStore;
use crate::transaction_signing_filter;
use move_bytecode_verifier::meter::BoundMeter;
use std::collections::{BTreeMap, HashSet};
use sui_adapter::adapter::default_verifier_config;
use sui_adapter::adapter::run_metered_move_bytecode_verifier;
use sui_config::transaction_deny_config::TransactionDenyConfig;
use sui_macros::checked_arithmetic;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::ObjectRef;
use sui_types::error::{UserInputError, UserInputResult};
use sui_types::messages::{
    TransactionKind, VerifiedExecutableTransaction, VersionedProtocolMessage,
};
use sui_types::{
    base_types::{SequenceNumber, SuiAddress},
    error::SuiResult,
    fp_ensure,
    gas::{SuiCostTable, SuiGasStatus},
    messages::{InputObjectKind, InputObjects, TransactionData, TransactionDataAPI},
    object::{Object, Owner},
};
use sui_types::{SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION};
use tracing::instrument;

checked_arithmetic! {

// Entry point for all checks related to gas.
// Called on both signing and execution.
// On success the gas part of the transaction (gas data and gas coins)
// is verified and good to go
async fn get_gas_status(
    objects: &[Object],
    gas: &[ObjectRef],
    epoch_store: &AuthorityPerEpochStore,
    transaction: &TransactionData,
) -> SuiResult<SuiGasStatus> {
    // Get the first coin (possibly the only one) and make it "the gas coin", then
    // keep track of all others that can contribute to gas (gas smashing).
    let gas_object_ref = gas.get(0).unwrap();
    // all other gas coins
    let more_gas_object_refs = gas[1..].to_vec();

    check_gas(
        objects,
        epoch_store,
        gas_object_ref,
        more_gas_object_refs,
        transaction.gas_budget(),
        transaction.gas_price(),
        transaction.kind(),
    )
    .await
}

#[instrument(level = "trace", skip_all)]
pub async fn check_transaction_input(
    store: &AuthorityStore,
    epoch_store: &AuthorityPerEpochStore,
    transaction: &TransactionData,
    transaction_deny_config: &TransactionDenyConfig,
) -> SuiResult<(SuiGasStatus, InputObjects)> {
    transaction.check_version_supported(epoch_store.protocol_config())?;
    transaction.validity_check(epoch_store.protocol_config())?;
    check_non_system_packages_to_be_published(transaction, epoch_store.protocol_config())?;
    let input_objects = transaction.input_objects()?;
    transaction_signing_filter::check_transaction_for_signing(
        transaction,
        &input_objects,
        transaction_deny_config,
        store,
    )?;
    let objects = store.check_input_objects(&input_objects, epoch_store.protocol_config())?;
    let gas_status = get_gas_status(&objects, transaction.gas(), epoch_store, transaction).await?;
    let input_objects = check_objects(transaction, input_objects, objects)?;
    Ok((gas_status, input_objects))
}

pub async fn check_transaction_input_with_given_gas(
    store: &AuthorityStore,
    epoch_store: &AuthorityPerEpochStore,
    transaction: &TransactionData,
    gas_object: Object,
) -> SuiResult<(SuiGasStatus, InputObjects)> {
    transaction.check_version_supported(epoch_store.protocol_config())?;
    transaction.validity_check_no_gas_check(epoch_store.protocol_config())?;
    check_non_system_packages_to_be_published(transaction, epoch_store.protocol_config())?;
    let mut input_objects = transaction.input_objects()?;
    let mut objects = store.check_input_objects(&input_objects, epoch_store.protocol_config())?;

    let gas_object_ref = gas_object.compute_object_reference();
    input_objects.push(InputObjectKind::ImmOrOwnedMoveObject(gas_object_ref));
    objects.push(gas_object);

    let gas_status = get_gas_status(&objects, &[gas_object_ref], epoch_store, transaction).await?;
    let input_objects = check_objects(transaction, input_objects, objects)?;
    Ok((gas_status, input_objects))
}

/// WARNING! This should only be used for the dev-inspect transaction. This transaction type
/// bypasses many of the normal object checks
pub(crate) async fn check_dev_inspect_input(
    store: &AuthorityStore,
    config: &ProtocolConfig,
    kind: &TransactionKind,
    gas_object: Object,
) -> Result<(ObjectRef, InputObjects), anyhow::Error> {
    let gas_object_ref = gas_object.compute_object_reference();
    kind.validity_check(config)?;
    match kind {
        TransactionKind::ProgrammableTransaction(_) => (),
        TransactionKind::ChangeEpoch(_)
        | TransactionKind::Genesis(_)
        | TransactionKind::ConsensusCommitPrologue(_) => {
            anyhow::bail!("Transaction kind {} is not supported in dev-inspect", kind)
        }
    }
    let mut input_objects = kind.input_objects()?;
    let mut objects = store.check_input_objects(&input_objects, config)?;
    let mut used_objects: HashSet<SuiAddress> = HashSet::new();
    for object in &objects {
        if !object.is_immutable() {
            fp_ensure!(
                used_objects.insert(object.id().into()),
                UserInputError::MutableObjectUsedMoreThanOnce {
                    object_id: object.id()
                }
                .into()
            );
        }
    }
    input_objects.push(InputObjectKind::ImmOrOwnedMoveObject(gas_object_ref));
    objects.push(gas_object);
    let input_objects = InputObjects::new(input_objects.into_iter().zip(objects).collect());
    Ok((gas_object_ref, input_objects))
}

pub async fn check_certificate_input(
    store: &AuthorityStore,
    epoch_store: &AuthorityPerEpochStore,
    cert: &VerifiedExecutableTransaction,
) -> SuiResult<(SuiGasStatus, InputObjects)> {
    let protocol_version = epoch_store.protocol_version();

    // This should not happen - validators should not have signed the txn in the first place.
    assert!(
        cert.data()
            .transaction_data()
            .check_version_supported(epoch_store.protocol_config())
            .is_ok(),
        "Certificate formed with unsupported message version {:?} for protocol version {:?}",
        cert.message_version(),
        protocol_version
    );

    let tx_data = &cert.data().intent_message().value;
    let input_object_kinds = tx_data.input_objects()?;
    let input_object_data = if tx_data.is_change_epoch_tx() {
        // When changing the epoch, we update a the system object, which is shared, without going
        // through sequencing, so we must bypass the sequence checks here.
        store.check_input_objects(&input_object_kinds, epoch_store.protocol_config())?
    } else {
        store.check_sequenced_input_objects(cert.digest(), &input_object_kinds, epoch_store)?
    };
    let gas_status =
        get_gas_status(&input_object_data, tx_data.gas(), epoch_store, tx_data).await?;
    let input_objects = check_objects(tx_data, input_object_kinds, input_object_data)?;
    Ok((gas_status, input_objects))
}

/// Check transaction gas data/info and gas coins consistency.
/// Return the gas status to be used for the lifecycle of the transaction.
#[instrument(level = "trace", skip_all)]
async fn check_gas(
    objects: &[Object],
    epoch_store: &AuthorityPerEpochStore,
    gas_payment: &ObjectRef,
    more_gas_object_refs: Vec<ObjectRef>,
    gas_budget: u64,
    gas_price: u64,
    tx_kind: &TransactionKind,
) -> SuiResult<SuiGasStatus> {
    let protocol_config = epoch_store.protocol_config();
    if tx_kind.is_system_tx() {
        Ok(SuiGasStatus::new_unmetered(protocol_config))
    } else {
        // gas price must be bigger or equal to reference gas price
        let reference_gas_price = epoch_store.reference_gas_price();
        if gas_price < reference_gas_price {
            return Err(UserInputError::GasPriceUnderRGP {
                gas_price,
                reference_gas_price,
            }
            .into());
        }
        if protocol_config.gas_model_version() >= 4 && gas_price >= protocol_config.max_gas_price() {
            return Err(UserInputError::GasPriceTooHigh {
                max_gas_price: protocol_config.max_gas_price(),
            }
            .into());
        }

        // load all gas coins
        let objects: BTreeMap<_, _> = objects.iter().map(|o| (o.id(), o)).collect();

        let gas_object = objects.get(&gas_payment.0);
        let gas_object = *gas_object.ok_or(UserInputError::ObjectNotFound {
            object_id: gas_payment.0,
            version: Some(gas_payment.1),
        })?;
        let mut more_gas_objects = vec![];
        for obj_ref in more_gas_object_refs.iter() {
            let obj = objects.get(&obj_ref.0);
            let obj = *obj.ok_or(UserInputError::ObjectNotFound {
                object_id: obj_ref.0,
                version: Some(obj_ref.1),
            })?;
            more_gas_objects.push(obj);
        }

        // check balance and coins consistency
        let cost_table = SuiCostTable::new(protocol_config);
        cost_table.check_gas_balance(gas_object, more_gas_objects, gas_budget, gas_price)?;
        Ok(SuiGasStatus::new_with_budget(
            gas_budget,
            gas_price,
            protocol_config,
        ))
    }
}

/// Check all the objects used in the transaction against the database, and ensure
/// that they are all the correct version and number.
#[instrument(level = "trace", skip_all)]
pub fn check_objects(
    transaction: &TransactionData,
    input_objects: Vec<InputObjectKind>,
    objects: Vec<Object>,
) -> UserInputResult<InputObjects> {
    // We require that mutable objects cannot show up more than once.
    let mut used_objects: HashSet<SuiAddress> = HashSet::new();
    for object in objects.iter() {
        if !object.is_immutable() {
            fp_ensure!(
                used_objects.insert(object.id().into()),
                UserInputError::MutableObjectUsedMoreThanOnce {
                    object_id: object.id()
                }
            );
        }
    }

    // Gather all objects and errors.
    let mut all_objects = Vec::with_capacity(input_objects.len());

    for (object_kind, object) in input_objects.into_iter().zip(objects) {
        // For Gas Object, we check the object is owned by gas owner
        // TODO: this is a quadratic check and though limits are low we should do it differently
        let owner_address = if transaction
            .gas()
            .iter()
            .any(|obj_ref| *obj_ref.0 == *object.id())
        {
            transaction.gas_owner()
        } else {
            transaction.sender()
        };
        // Check if the object contents match the type of lock we need for
        // this object.
        let system_transaction = transaction.is_system_tx();
        check_one_object(&owner_address, object_kind, &object, system_transaction)?;
        all_objects.push((object_kind, object));
    }
    if !transaction.is_genesis_tx() && all_objects.is_empty() {
        return Err(UserInputError::ObjectInputArityViolation);
    }

    Ok(InputObjects::new(all_objects))
}

/// Check one object against a reference
fn check_one_object(
    owner: &SuiAddress,
    object_kind: InputObjectKind,
    object: &Object,
    system_transaction: bool,
) -> UserInputResult {
    match object_kind {
        InputObjectKind::MovePackage(package_id) => {
            fp_ensure!(
                object.data.try_as_package().is_some(),
                UserInputError::MoveObjectAsPackage {
                    object_id: package_id
                }
            );
        }
        InputObjectKind::ImmOrOwnedMoveObject((object_id, sequence_number, object_digest)) => {
            fp_ensure!(
                !object.is_package(),
                UserInputError::MovePackageAsObject { object_id }
            );
            fp_ensure!(
                sequence_number < SequenceNumber::MAX,
                UserInputError::InvalidSequenceNumber
            );

            // This is an invariant - we just load the object with the given ID and version.
            assert_eq!(
                object.version(),
                sequence_number,
                "The fetched object version {} does not match the requested version {}, object id: {}",
                object.version(),
                sequence_number,
                object.id(),
            );

            // Check the digest matches - user could give a mismatched ObjectDigest
            let expected_digest = object.digest();
            fp_ensure!(
                expected_digest == object_digest,
                UserInputError::InvalidObjectDigest {
                    object_id,
                    expected_digest
                }
            );

            match object.owner {
                Owner::Immutable => {
                    // Nothing else to check for Immutable.
                }
                Owner::AddressOwner(actual_owner) => {
                    // Check the owner is correct.
                    fp_ensure!(
                        owner == &actual_owner,
                        UserInputError::IncorrectUserSignature {
                            error: format!("Object {:?} is owned by account address {:?}, but given owner/signer address is {:?}", object_id, actual_owner, owner),
                        }
                    );
                }
                Owner::ObjectOwner(owner) => {
                    return Err(UserInputError::InvalidChildObjectArgument {
                        child_id: object.id(),
                        parent_id: owner.into(),
                    });
                }
                Owner::Shared { .. } => {
                    // This object is a mutable shared object. However the transaction
                    // specifies it as an owned object. This is inconsistent.
                    return Err(UserInputError::NotSharedObjectError);
                }
            };
        }
        InputObjectKind::SharedMoveObject {
            id: SUI_CLOCK_OBJECT_ID,
            initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
            mutable: true,
        } => {
            // Only system transactions can accept the Clock
            // object as a mutable parameter.
            if system_transaction {
                return Ok(());
            } else {
                return Err(UserInputError::ImmutableParameterExpectedError {
                    object_id: SUI_CLOCK_OBJECT_ID,
                });
            }
        }
        InputObjectKind::SharedMoveObject {
            initial_shared_version: input_initial_shared_version,
            ..
        } => {
            fp_ensure!(
                object.version() < SequenceNumber::MAX,
                UserInputError::InvalidSequenceNumber
            );

            match object.owner {
                Owner::AddressOwner(_) | Owner::ObjectOwner(_) | Owner::Immutable => {
                    // When someone locks an object as shared it must be shared already.
                    return Err(UserInputError::NotSharedObjectError);
                }
                Owner::Shared {
                    initial_shared_version: actual_initial_shared_version,
                } => {
                    fp_ensure!(
                        input_initial_shared_version == actual_initial_shared_version,
                        UserInputError::SharedObjectStartingVersionMismatch
                    )
                }
            }
        }
    };
    Ok(())
}

/// Check package verification timeout
#[instrument(level = "trace", skip_all)]
pub fn check_non_system_packages_to_be_published(
    transaction: &TransactionData,
    protocol_config: &ProtocolConfig,
) -> UserInputResult<()> {
    // Only meter non-system TXes
    if !transaction.is_system_tx() {
        // We use a custom config with metering enabled
        let metered_verifier_config =
            default_verifier_config(protocol_config, true /* enable metering */);
        // Use the same meter for all packages
        let mut meter = BoundMeter::new(&metered_verifier_config);
        if let TransactionKind::ProgrammableTransaction(pt) = transaction.kind() {
            pt.non_system_packages_to_be_published()
            .try_for_each(|q| run_metered_move_bytecode_verifier(q, protocol_config, &metered_verifier_config, &mut meter))
            .map_err(|e| UserInputError::PackageVerificationTimedout { err: e.to_string() })?;
        }
    }

    Ok(())
}

}
