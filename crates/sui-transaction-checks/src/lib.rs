// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod deny;

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
mod checked {
    use once_cell::sync::OnceCell;
    use std::collections::{BTreeMap, HashMap, HashSet};
    use std::sync::Arc;
    use sui_config::transaction_deny_config::TransactionDenyConfig;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::base_types::{ObjectID, ObjectRef};
    use sui_types::committee::EpochId;
    use sui_types::digests::TransactionDigest;
    use sui_types::error::{UserInputError, UserInputResult};
    use sui_types::executable_transaction::VerifiedExecutableTransaction;
    use sui_types::metrics::BytecodeVerifierMetrics;
    use sui_types::signature::GenericSignature;
    use sui_types::storage::ObjectStore;
    use sui_types::storage::ReceivedMarkerQuery;
    use sui_types::storage::{BackingPackageStore, GetSharedLocks};
    use sui_types::transaction::{
        InputObjectKind, InputObjects, TransactionData, TransactionDataAPI, TransactionKind,
        VersionedProtocolMessage,
    };
    use sui_types::{
        base_types::{SequenceNumber, SuiAddress},
        error::{SuiError, SuiResult},
        fp_bail, fp_ensure,
        gas::SuiGasStatus,
        object::{Object, Owner},
    };
    use sui_types::{
        SUI_AUTHENTICATOR_STATE_OBJECT_ID, SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION,
    };
    use tracing::error;
    use tracing::instrument;

    // Entry point for all checks related to gas.
    // Called on both signing and execution.
    // On success the gas part of the transaction (gas data and gas coins)
    // is verified and good to go
    pub fn get_gas_status(
        objects: &[Object],
        gas: &[ObjectRef],
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
        transaction: &TransactionData,
    ) -> SuiResult<SuiGasStatus> {
        check_gas(
            objects,
            protocol_config,
            reference_gas_price,
            gas,
            transaction.gas_budget(),
            transaction.gas_price(),
            transaction.kind(),
        )
    }

    #[instrument(level = "trace", skip_all)]
    pub fn check_transaction_input<S: BackingPackageStore + ObjectStore + ReceivedMarkerQuery>(
        store: &S,
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
        epoch_id: EpochId,
        transaction: &TransactionData,
        tx_signatures: &[GenericSignature],
        transaction_deny_config: &TransactionDenyConfig,
        metrics: &Arc<BytecodeVerifierMetrics>,
    ) -> SuiResult<(SuiGasStatus, InputObjects)> {
        transaction.check_version_supported(protocol_config)?;
        transaction.validity_check(protocol_config)?;
        let receiving_objects = transaction.receiving_objects();
        let input_objects = transaction.input_objects()?;
        crate::deny::check_transaction_for_signing(
            transaction,
            tx_signatures,
            &input_objects,
            &receiving_objects,
            transaction_deny_config,
            &store,
        )?;

        // Runs verifier, which could be expensive.
        check_non_system_packages_to_be_published(transaction, protocol_config, metrics)?;

        let objects = check_input_objects(store, &input_objects, protocol_config)?;
        let gas_status = get_gas_status(
            &objects,
            transaction.gas(),
            protocol_config,
            reference_gas_price,
            transaction,
        )?;
        let input_objects = check_objects(transaction, input_objects, objects)?;
        check_receiving_objects(
            store,
            &receiving_objects,
            &input_objects,
            protocol_config,
            epoch_id,
        )?;
        Ok((gas_status, input_objects))
    }

    pub fn check_transaction_input_with_given_gas<S: ObjectStore + ReceivedMarkerQuery>(
        store: &S,
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
        epoch_id: EpochId,
        transaction: &TransactionData,
        gas_object: Object,
        metrics: &Arc<BytecodeVerifierMetrics>,
    ) -> SuiResult<(SuiGasStatus, InputObjects)> {
        transaction.check_version_supported(protocol_config)?;
        transaction.validity_check_no_gas_check(protocol_config)?;
        check_non_system_packages_to_be_published(transaction, protocol_config, metrics)?;
        let receiving_objects = transaction.receiving_objects();
        let mut input_objects = transaction.input_objects()?;
        let mut objects = check_input_objects(store, &input_objects, protocol_config)?;

        let gas_object_ref = gas_object.compute_object_reference();
        input_objects.push(InputObjectKind::ImmOrOwnedMoveObject(gas_object_ref));
        objects.push(gas_object);

        let gas_status = get_gas_status(
            &objects,
            &[gas_object_ref],
            protocol_config,
            reference_gas_price,
            transaction,
        )?;
        let input_objects = check_objects(transaction, input_objects, objects)?;
        check_receiving_objects(
            store,
            &receiving_objects,
            &input_objects,
            protocol_config,
            epoch_id,
        )?;
        Ok((gas_status, input_objects))
    }

    #[instrument(level = "trace", skip_all)]
    pub fn check_certificate_input<S: ObjectStore, G: GetSharedLocks>(
        store: &S,
        shared_lock_store: &G,
        cert: &VerifiedExecutableTransaction,
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
    ) -> SuiResult<(SuiGasStatus, InputObjects)> {
        // This should not happen - validators should not have signed the txn in the first place.
        assert!(
            cert.data()
                .transaction_data()
                .check_version_supported(protocol_config)
                .is_ok(),
            "Certificate formed with unsupported message version {:?}",
            cert.message_version(),
        );

        let tx_data = &cert.data().intent_message().value;
        let input_object_kinds = tx_data.input_objects()?;
        let input_object_data = if tx_data.is_end_of_epoch_tx() {
            // When changing the epoch, we update a the system object, which is shared, without going
            // through sequencing, so we must bypass the sequence checks here.
            check_input_objects(store, &input_object_kinds, protocol_config)?
        } else {
            check_sequenced_input_objects(
                store,
                cert.digest(),
                &input_object_kinds,
                shared_lock_store,
            )?
        };
        let gas_status = get_gas_status(
            &input_object_data,
            tx_data.gas(),
            protocol_config,
            reference_gas_price,
            tx_data,
        )?;
        let input_objects = check_objects(tx_data, input_object_kinds, input_object_data)?;
        // NB: We do not check receiving objects when executing. Only at signing time do we check.
        Ok((gas_status, input_objects))
    }

    /// WARNING! This should only be used for the dev-inspect transaction. This transaction type
    /// bypasses many of the normal object checks
    pub fn check_dev_inspect_input<S: ObjectStore>(
        store: &S,
        config: &ProtocolConfig,
        kind: &TransactionKind,
        gas_object: Object,
    ) -> SuiResult<(ObjectRef, InputObjects)> {
        let gas_object_ref = gas_object.compute_object_reference();
        kind.validity_check(config)?;
        if kind.is_system_tx() {
            return Err(UserInputError::Unsupported(format!(
                "Transaction kind {} is not supported in dev-inspect",
                kind
            ))
            .into());
        }
        let mut input_objects = kind.input_objects()?;
        let mut objects = check_input_objects(store, &input_objects, config)?;
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

    fn check_receiving_objects<S: ObjectStore + ReceivedMarkerQuery>(
        store: &S,
        receiving_objects: &[ObjectRef],
        input_objects: &InputObjects,
        protocol_config: &ProtocolConfig,
        epoch_id: EpochId,
    ) -> Result<(), SuiError> {
        // Count receiving objects towards the input object limit as they are passed in the PTB
        // args and they will (most likely) incur an object load at runtime.
        fp_ensure!(
            receiving_objects.len() + input_objects.len()
                <= protocol_config.max_input_objects() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum input and receiving objects in a transaction".to_string(),
                value: protocol_config.max_input_objects().to_string()
            }
            .into()
        );

        let mut objects_in_txn: HashSet<_> = input_objects
            .object_kinds()
            .map(|x| x.object_id())
            .collect();

        // Since we're at signing we check that every object reference that we are receiving is the
        // most recent version of that object. If it's been received at the version specified we
        // let it through to allow the transaction to run and fail to unlock any other objects in
        // the transaction. Otherwise, we return an error.
        //
        // If there are any object IDs in common (either between receiving objects and input
        // objects) we return an error.
        for (object_id, version, object_digest) in receiving_objects {
            fp_ensure!(
                *version < SequenceNumber::MAX,
                UserInputError::InvalidSequenceNumber.into()
            );

            let object = store.get_object(object_id)?;

            if !object.as_ref().is_some_and(|x| {
                x.owner.is_address_owned()
                    && x.version() == *version
                    && x.digest() == *object_digest
            }) && !store.have_received_object_at_version(object_id, *version, epoch_id)?
            {
                // Unable to load object
                fp_ensure!(
                    object.is_some(),
                    UserInputError::ObjectNotFound {
                        object_id: *object_id,
                        version: Some(*version),
                    }
                    .into()
                );

                let object = object.expect("Safe to unwrap due to check in fp_unwrap");

                // Version mismatch
                fp_ensure!(
                    object.version() == *version,
                    UserInputError::ObjectVersionUnavailableForConsumption {
                        provided_obj_ref: (*object_id, *version, *object_digest),
                        current_version: object.version(),
                    }
                    .into()
                );

                // Tried to receive a package
                fp_ensure!(
                    !object.is_package(),
                    UserInputError::MovePackageAsObject {
                        object_id: *object_id
                    }
                    .into()
                );

                // Digest mismatch
                let expected_digest = object.digest();
                fp_ensure!(
                    expected_digest == *object_digest,
                    UserInputError::InvalidObjectDigest {
                        object_id: *object_id,
                        expected_digest
                    }
                    .into()
                );

                match object.owner {
                    Owner::AddressOwner(_) => {
                        debug_assert!(false,
                            "Receiving object {:?} is invalid but we expect it should be valid. {:?}",
                            (*object_id, *version, *object_id), object
                        );
                        error!(
                            "Receiving object {:?} is invalid but we expect it should be valid. {:?}",
                            (*object_id, *version, *object_id), object
                        );
                        // We should never get here, but if for some reason we do just default to
                        // object not found and reject signing the transaction.
                        fp_bail!(UserInputError::ObjectNotFound {
                            object_id: *object_id,
                            version: Some(*version),
                        }
                        .into())
                    }
                    Owner::ObjectOwner(owner) => {
                        fp_bail!(UserInputError::InvalidChildObjectArgument {
                            child_id: object.id(),
                            parent_id: owner.into(),
                        }
                        .into())
                    }
                    Owner::Shared { .. } => fp_bail!(UserInputError::NotSharedObjectError.into()),
                    Owner::Immutable => fp_bail!(UserInputError::MutableParameterExpected {
                        object_id: *object_id
                    }
                    .into()),
                };
            }

            fp_ensure!(
                !objects_in_txn.contains(object_id),
                UserInputError::DuplicateObjectRefInput.into()
            );

            objects_in_txn.insert(*object_id);
        }
        Ok(())
    }

    pub fn check_input_objects<S: ObjectStore>(
        object_store: &S,
        objects: &[InputObjectKind],
        protocol_config: &ProtocolConfig,
    ) -> Result<Vec<Object>, SuiError> {
        let mut result = Vec::new();

        fp_ensure!(
            objects.len() <= protocol_config.max_input_objects() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum input objects in a transaction".to_string(),
                value: protocol_config.max_input_objects().to_string()
            }
            .into()
        );

        for kind in objects {
            let obj = match kind {
                InputObjectKind::MovePackage(id) | InputObjectKind::SharedMoveObject { id, .. } => {
                    object_store.get_object(id)?
                }
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    object_store.get_object_by_key(&objref.0, objref.1)?
                }
            }
            .ok_or_else(|| SuiError::from(kind.object_not_found_error()))?;
            result.push(obj);
        }
        Ok(result)
    }

    /// When making changes, please see if get_input_object_keys() above needs
    /// similar changes as well.
    ///
    /// Before this function is invoked, TransactionManager must ensure all depended
    /// objects are present. Thus any missing object will panic.
    fn check_sequenced_input_objects<S: ObjectStore, G: GetSharedLocks>(
        store: &S,
        digest: &TransactionDigest,
        objects: &[InputObjectKind],
        shared_locks_store: &G,
    ) -> Result<Vec<Object>, SuiError> {
        let shared_locks_cell: OnceCell<HashMap<_, _>> = OnceCell::new();

        let mut result = Vec::new();
        for kind in objects {
            let obj = match kind {
                InputObjectKind::SharedMoveObject { id, .. } => {
                    let shared_locks = shared_locks_cell.get_or_try_init(|| {
                        Ok::<HashMap<ObjectID, SequenceNumber>, SuiError>(
                            shared_locks_store.get_shared_locks(digest)?.into_iter().collect(),
                        )
                    })?;
                    // If we can't find the locked version, it means
                    // 1. either we have a bug that skips shared object version assignment
                    // 2. or we have some DB corruption
                    let version = shared_locks.get(id).unwrap_or_else(|| {
                        panic!(
                            "Shared object locks should have been set. tx_digset: {:?}, obj id: {:?}",
                            digest, id
                        )
                    });
                    store.get_object_by_key(id, *version)?.unwrap_or_else(|| {
                        panic!("All dependencies of tx {:?} should have been executed now, but Shared Object id: {}, version: {} is absent", digest, *id, *version);
                    })
                }
                InputObjectKind::MovePackage(id) => store.get_object(id)?.unwrap_or_else(|| {
                    panic!("All dependencies of tx {:?} should have been executed now, but Move Package id: {} is absent", digest, id);
                }),
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    store.get_object_by_key(&objref.0, objref.1)?.unwrap_or_else(|| {
                        panic!("All dependencies of tx {:?} should have been executed now, but Immutable or Owned Object id: {}, version: {} is absent", digest, objref.0, objref.1);
                    })
                }
            };
            result.push(obj);
        }
        Ok(result)
    }

    /// Check transaction gas data/info and gas coins consistency.
    /// Return the gas status to be used for the lifecycle of the transaction.
    #[instrument(level = "trace", skip_all)]
    fn check_gas(
        objects: &[Object],
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
        gas: &[ObjectRef],
        gas_budget: u64,
        gas_price: u64,
        tx_kind: &TransactionKind,
    ) -> SuiResult<SuiGasStatus> {
        if tx_kind.is_system_tx() {
            Ok(SuiGasStatus::new_unmetered())
        } else {
            let gas_status =
                SuiGasStatus::new(gas_budget, gas_price, reference_gas_price, protocol_config)?;

            // check balance and coins consistency
            // load all gas coins
            let objects: BTreeMap<_, _> = objects.iter().map(|o| (o.id(), o)).collect();
            let mut gas_objects = vec![];
            for obj_ref in gas {
                let obj = objects.get(&obj_ref.0);
                let obj = *obj.ok_or(UserInputError::ObjectNotFound {
                    object_id: obj_ref.0,
                    version: Some(obj_ref.1),
                })?;
                gas_objects.push(obj);
            }
            gas_status.check_gas_balance(&gas_objects, gas_budget)?;
            Ok(gas_status)
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
                id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
                ..
            } => {
                if system_transaction {
                    return Ok(());
                } else {
                    return Err(UserInputError::InaccessibleSystemObject {
                        object_id: SUI_AUTHENTICATOR_STATE_OBJECT_ID,
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
        metrics: &Arc<BytecodeVerifierMetrics>,
    ) -> UserInputResult<()> {
        // Only meter non-system programmable transaction blocks
        if transaction.is_system_tx() {
            return Ok(());
        }

        let TransactionKind::ProgrammableTransaction(pt) = transaction.kind() else {
            return Ok(());
        };

        // We use a custom config with metering enabled
        let is_metered = true;
        // Use the same verifier and meter for all packages
        let mut verifier = sui_execution::verifier(protocol_config, is_metered, metrics);

        // Measure time for verifying all packages in the PTB
        let shared_meter_verifier_timer = metrics
            .verifier_runtime_per_ptb_success_latency
            .start_timer();

        let verifier_status = pt
            .non_system_packages_to_be_published()
            .try_for_each(|module_bytes| verifier.meter_module_bytes(protocol_config, module_bytes))
            .map_err(|e| UserInputError::PackageVerificationTimedout { err: e.to_string() });

        match verifier_status {
            Ok(_) => {
                // Success: stop and record the success timer
                shared_meter_verifier_timer.stop_and_record();
            }
            Err(err) => {
                // Failure: redirect the success timers output to the failure timer and
                // discard the success timer
                metrics
                    .verifier_runtime_per_ptb_timeout_latency
                    .observe(shared_meter_verifier_timer.stop_and_discard());
                return Err(err);
            }
        };

        Ok(())
    }
}
