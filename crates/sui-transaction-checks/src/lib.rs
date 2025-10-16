// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod deny;

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
mod checked {
    use move_binary_format::binary_config::BinaryConfig;
    use move_binary_format::CompiledModule;
    use move_bytecode_verifier_latest::verify_module_with_config_metered;
    use move_bytecode_verifier_meter::{Meter, Scope};
    use move_vm_config::verifier::VerifierConfig;
    use std::collections::{BTreeMap, HashSet};
    use std::sync::Arc;
    use sui_config::verifier_signing_config::VerifierSigningConfig;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::base_types::{ObjectID, ObjectRef};
    use sui_types::error::{SuiResult, UserInputError, UserInputResult};
    use sui_types::executable_transaction::VerifiedExecutableTransaction;
    use sui_types::execution_config_utils::to_binary_config;
    use sui_types::metrics::BytecodeVerifierMetrics;
    use sui_types::transaction::{
        CheckedInputObjects, InputObjectKind, InputObjects, ObjectReadResult, ObjectReadResultKind,
        ReceivingObjectReadResult, ReceivingObjects, TransactionData, TransactionDataAPI,
        TransactionKind,
    };
    use sui_types::{
        base_types::{SequenceNumber, SuiAddress},
        error::SuiError,
        fp_bail, fp_ensure,
        gas::SuiGasStatus,
        object::{Object, Owner},
    };
    use sui_types::{
        SUI_AUTHENTICATOR_STATE_OBJECT_ID, SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION,
        SUI_RANDOMNESS_STATE_OBJECT_ID,
    };
    use sui_verifier::check_for_verifier_timeout;
    use sui_verifier::meter::SuiVerifierMeter;
    use sui_verifier::verifier::sui_verify_module_metered_check_timeout_only;
    use tracing::error;
    use tracing::instrument;

    trait IntoChecked {
        fn into_checked(self) -> CheckedInputObjects;
    }

    impl IntoChecked for InputObjects {
        fn into_checked(self) -> CheckedInputObjects {
            CheckedInputObjects::new_with_checked_transaction_inputs(self)
        }
    }

    // Entry point for all checks related to gas.
    // Called on both signing and execution.
    // On success the gas part of the transaction (gas data and gas coins)
    // is verified and good to go
    pub fn get_gas_status(
        objects: &InputObjects,
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
    pub fn check_transaction_input(
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
        transaction: &TransactionData,
        input_objects: InputObjects,
        receiving_objects: &ReceivingObjects,
        metrics: &Arc<BytecodeVerifierMetrics>,
        verifier_signing_config: &VerifierSigningConfig,
    ) -> SuiResult<(SuiGasStatus, CheckedInputObjects)> {
        let gas_status = check_transaction_input_inner(
            protocol_config,
            reference_gas_price,
            transaction,
            &input_objects,
            &[],
        )?;
        check_receiving_objects(&input_objects, receiving_objects)?;
        // Runs verifier, which could be expensive.
        check_non_system_packages_to_be_published(
            transaction,
            protocol_config,
            metrics,
            verifier_signing_config,
        )?;

        Ok((gas_status, input_objects.into_checked()))
    }

    pub fn check_transaction_input_with_given_gas(
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
        transaction: &TransactionData,
        mut input_objects: InputObjects,
        receiving_objects: ReceivingObjects,
        gas_object: Object,
        metrics: &Arc<BytecodeVerifierMetrics>,
        verifier_signing_config: &VerifierSigningConfig,
    ) -> SuiResult<(SuiGasStatus, CheckedInputObjects)> {
        let gas_object_ref = gas_object.compute_object_reference();
        input_objects.push(ObjectReadResult::new_from_gas_object(&gas_object));

        let gas_status = check_transaction_input_inner(
            protocol_config,
            reference_gas_price,
            transaction,
            &input_objects,
            &[gas_object_ref],
        )?;
        check_receiving_objects(&input_objects, &receiving_objects)?;
        // Runs verifier, which could be expensive.
        check_non_system_packages_to_be_published(
            transaction,
            protocol_config,
            metrics,
            verifier_signing_config,
        )?;

        Ok((gas_status, input_objects.into_checked()))
    }

    // Since the purpose of this function is to audit certified transactions,
    // the checks here should be a strict subset of the checks in check_transaction_input().
    // For checks not performed in this function but in check_transaction_input(),
    // we should add a comment calling out the difference.
    #[instrument(level = "trace", skip_all)]
    pub fn check_certificate_input(
        cert: &VerifiedExecutableTransaction,
        input_objects: InputObjects,
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
    ) -> SuiResult<(SuiGasStatus, CheckedInputObjects)> {
        let transaction = cert.data().transaction_data();
        let gas_status = check_transaction_input_inner(
            protocol_config,
            reference_gas_price,
            transaction,
            &input_objects,
            &[],
        )?;
        // NB: We do not check receiving objects when executing. Only at signing time do we check.
        // NB: move verifier is only checked at signing time, not at execution.

        Ok((gas_status, input_objects.into_checked()))
    }

    /// WARNING! This should only be used for the dev-inspect transaction. This transaction type
    /// bypasses many of the normal object checks
    pub fn check_dev_inspect_input(
        config: &ProtocolConfig,
        kind: &TransactionKind,
        input_objects: InputObjects,
        // TODO: check ReceivingObjects for dev inspect?
        _receiving_objects: ReceivingObjects,
    ) -> SuiResult<CheckedInputObjects> {
        kind.validity_check(config)?;
        if kind.is_system_tx() {
            return Err(UserInputError::Unsupported(format!(
                "Transaction kind {} is not supported in dev-inspect",
                kind
            ))
            .into());
        }
        let mut used_objects: HashSet<SuiAddress> = HashSet::new();
        for input_object in input_objects.iter() {
            let Some(object) = input_object.as_object() else {
                // object was deleted
                continue;
            };

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

        Ok(input_objects.into_checked())
    }

    // Common checks performed for transactions and certificates.
    fn check_transaction_input_inner(
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
        transaction: &TransactionData,
        input_objects: &InputObjects,
        // Overrides the gas objects in the transaction.
        gas_override: &[ObjectRef],
    ) -> SuiResult<SuiGasStatus> {
        let gas = if gas_override.is_empty() {
            transaction.gas()
        } else {
            gas_override
        };

        let gas_status = get_gas_status(
            input_objects,
            gas,
            protocol_config,
            reference_gas_price,
            transaction,
        )?;
        check_objects(transaction, input_objects)?;

        Ok(gas_status)
    }

    fn check_receiving_objects(
        input_objects: &InputObjects,
        receiving_objects: &ReceivingObjects,
    ) -> Result<(), SuiError> {
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
        for ReceivingObjectReadResult {
            object_ref: (object_id, version, object_digest),
            object,
        } in receiving_objects.iter()
        {
            fp_ensure!(
                *version < SequenceNumber::MAX,
                UserInputError::InvalidSequenceNumber.into()
            );

            let Some(object) = object.as_object() else {
                // object was previously received
                continue;
            };

            if !(object.owner.is_address_owned()
                && object.version() == *version
                && object.digest() == *object_digest)
            {
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
                    Owner::Shared { .. } | Owner::ConsensusAddressOwner { .. } => {
                        fp_bail!(UserInputError::NotSharedObjectError.into())
                    }
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

    /// Check transaction gas data/info and gas coins consistency.
    /// Return the gas status to be used for the lifecycle of the transaction.
    #[instrument(level = "trace", skip_all)]
    fn check_gas(
        objects: &InputObjects,
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
    fn check_objects(transaction: &TransactionData, objects: &InputObjects) -> UserInputResult<()> {
        // We require that mutable objects cannot show up more than once.
        let mut used_objects: HashSet<SuiAddress> = HashSet::new();
        for object in objects.iter() {
            if object.is_mutable() {
                fp_ensure!(
                    used_objects.insert(object.id().into()),
                    UserInputError::MutableObjectUsedMoreThanOnce {
                        object_id: object.id()
                    }
                );
            }
        }

        if !transaction.is_genesis_tx() && objects.is_empty() {
            return Err(UserInputError::ObjectInputArityViolation);
        }

        let gas_coins: HashSet<ObjectID> =
            HashSet::from_iter(transaction.gas().iter().map(|obj_ref| obj_ref.0));
        for object in objects.iter() {
            let input_object_kind = object.input_object_kind;

            match &object.object {
                ObjectReadResultKind::Object(object) => {
                    // For Gas Object, we check the object is owned by gas owner
                    let owner_address = if gas_coins.contains(&object.id()) {
                        transaction.gas_owner()
                    } else {
                        transaction.sender()
                    };
                    // Check if the object contents match the type of lock we need for
                    // this object.
                    let system_transaction = transaction.is_system_tx();
                    check_one_object(
                        &owner_address,
                        input_object_kind,
                        object,
                        system_transaction,
                    )?;
                }
                // We skip checking a removed consensus object because it no longer exists.
                ObjectReadResultKind::ObjectConsensusStreamEnded(_, _) => (),
                // We skip checking shared objects from cancelled transactions since we are not reading it.
                ObjectReadResultKind::CancelledTransactionSharedObject(_) => (),
            }
        }

        Ok(())
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
                            error: format!("Object {object_id:?} is owned by account address {actual_owner:?}, but given owner/signer address is {owner:?}"),
                        }
                    );
                    }
                    Owner::ObjectOwner(owner) => {
                        return Err(UserInputError::InvalidChildObjectArgument {
                            child_id: object.id(),
                            parent_id: owner.into(),
                        });
                    }
                    Owner::Shared { .. } | Owner::ConsensusAddressOwner { .. } => {
                        // This object is a mutable consensus object. However the transaction
                        // specifies it as an owned object. This is inconsistent.
                        return Err(UserInputError::NotOwnedObjectError);
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
                id: SUI_RANDOMNESS_STATE_OBJECT_ID,
                mutable: true,
                ..
            } => {
                // Only system transactions can accept the Random
                // object as a mutable parameter.
                if system_transaction {
                    return Ok(());
                } else {
                    return Err(UserInputError::ImmutableParameterExpectedError {
                        object_id: SUI_RANDOMNESS_STATE_OBJECT_ID,
                    });
                }
            }
            InputObjectKind::SharedMoveObject {
                id: object_id,
                initial_shared_version: input_initial_shared_version,
                ..
            } => {
                fp_ensure!(
                    object.version() < SequenceNumber::MAX,
                    UserInputError::InvalidSequenceNumber
                );

                match &object.owner {
                    Owner::AddressOwner(_) | Owner::ObjectOwner(_) | Owner::Immutable => {
                        // When someone locks an object as shared it must be shared already.
                        return Err(UserInputError::NotSharedObjectError);
                    }
                    Owner::Shared {
                        initial_shared_version: actual_initial_shared_version,
                    } => {
                        fp_ensure!(
                            input_initial_shared_version == *actual_initial_shared_version,
                            UserInputError::SharedObjectStartingVersionMismatch
                        )
                    }
                    Owner::ConsensusAddressOwner {
                        start_version: actual_initial_shared_version,
                        owner: actual_owner,
                    } => {
                        fp_ensure!(
                            input_initial_shared_version == *actual_initial_shared_version,
                            UserInputError::SharedObjectStartingVersionMismatch
                        );
                        // Check the owner is correct.
                        fp_ensure!(
                            owner == actual_owner,
                            UserInputError::IncorrectUserSignature {
                                error: format!("Object {object_id:?} is owned by account address {actual_owner:?}, but given owner/signer address is {owner:?}"),
                            }
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
        verifier_signing_config: &VerifierSigningConfig,
    ) -> UserInputResult<()> {
        // Only meter non-system programmable transaction blocks
        if transaction.is_system_tx() {
            return Ok(());
        }

        let TransactionKind::ProgrammableTransaction(pt) = transaction.kind() else {
            return Ok(());
        };

        // Use the same verifier and meter for all packages, custom configured for signing.
        let signing_limits = Some(verifier_signing_config.limits_for_signing());
        let meter = &mut SuiVerifierMeter::new(verifier_signing_config.meter_config_for_signing());

        // Measure time for verifying all packages in the PTB
        let shared_meter_verifier_timer = metrics
            .verifier_runtime_per_ptb_success_latency
            .start_timer();

        let binary_config = to_binary_config(protocol_config);
        let verifier_config = protocol_config.verifier_config(signing_limits);
        let verifier_status = pt
            .non_system_packages_to_be_published()
            .try_for_each(|module_bytes| {
                metered_verify_module_bytes(
                    &binary_config,
                    &verifier_config,
                    module_bytes,
                    meter,
                    metrics,
                )
            })
            .map_err(|e| UserInputError::PackageVerificationTimeout { err: e.to_string() });

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

    fn metered_verify_module_bytes(
        binary_config: &BinaryConfig,
        verifier_config: &VerifierConfig,
        module_bytes: &[Vec<u8>],
        meter: &mut dyn Meter,
        metrics: &Arc<BytecodeVerifierMetrics>,
    ) -> SuiResult<()> {
        let Ok(modules) = module_bytes
            .iter()
            .map(|b| CompiledModule::deserialize_with_config(b, binary_config))
            .collect::<Result<Vec<_>, _>>()
        else {
            // Although we failed, we don't care since it wasn't because of a timeout.
            return Ok(());
        };

        for module in &modules {
            for identifier in module.identifiers() {
                if identifier.as_str() == "<SELF>" {
                    return Err(sui_types::error::UserInputError::InvalidIdentifier {
                        error: format!("invalid identifier: {}", identifier),
                    }
                    .into());
                }
            }
        }

        metered_verify_compiled_modules(verifier_config, &modules, meter, metrics)
    }

    /// Run the bytecode verifier with a meter limit
    ///
    /// This function only fails if the verification does not complete within the limit.  If the
    /// modules fail to verify but verification completes within the meter limit, the function
    /// succeeds.
    #[instrument(level = "trace", skip_all)]
    pub fn metered_verify_compiled_modules(
        verifier_config: &VerifierConfig,
        modules: &[CompiledModule],
        meter: &mut (impl Meter + ?Sized),
        metrics: &Arc<BytecodeVerifierMetrics>,
    ) -> Result<(), SuiError> {
        // run the Move verifier
        for module in modules.iter() {
            let per_module_meter_verifier_timer = metrics
                .verifier_runtime_per_module_success_latency
                .start_timer();

            if let Err(e) = verify_module_timeout_only(module, verifier_config, meter) {
                // We only checked that the failure was due to timeout
                // Discard success timer, but record timeout/failure timer
                metrics
                    .verifier_runtime_per_module_timeout_latency
                    .observe(per_module_meter_verifier_timer.stop_and_discard());
                metrics
                    .verifier_timeout_metrics
                    .with_label_values(&[
                        BytecodeVerifierMetrics::OVERALL_TAG,
                        BytecodeVerifierMetrics::TIMEOUT_TAG,
                    ])
                    .inc();

                return Err(e);
            };

            // Save the success timer
            per_module_meter_verifier_timer.stop_and_record();
            metrics
                .verifier_timeout_metrics
                .with_label_values(&[
                    BytecodeVerifierMetrics::OVERALL_TAG,
                    BytecodeVerifierMetrics::SUCCESS_TAG,
                ])
                .inc();
        }

        Ok(())
    }

    /// Run both the Move verifier and the Sui verifier, checking just for timeouts. Returns Ok(())
    /// if the verifier completes within the module meter limit and the ticks are successfully
    /// transfered to the package limit (regardless of whether verification succeeds or not).
    fn verify_module_timeout_only(
        module: &CompiledModule,
        verifier_config: &VerifierConfig,
        meter: &mut (impl Meter + ?Sized),
    ) -> Result<(), SuiError> {
        meter.enter_scope(module.self_id().name().as_str(), Scope::Module);

        if let Err(e) = verify_module_with_config_metered(verifier_config, module, meter) {
            // Check that the status indicates metering timeout.
            if check_for_verifier_timeout(&e.major_status()) {
                return Err(SuiError::ModuleVerificationFailure {
                    error: format!("Verification timed out: {}", e),
                });
            }
        } else if let Err(err) = sui_verify_module_metered_check_timeout_only(
            module,
            &BTreeMap::new(),
            meter,
            verifier_config,
        ) {
            return Err(err.into());
        }

        if meter.transfer(Scope::Module, Scope::Package, 1.0).is_err() {
            return Err(SuiError::ModuleVerificationFailure {
                error: "Verification timed out".to_string(),
            });
        }

        Ok(())
    }
}
