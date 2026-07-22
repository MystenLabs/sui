// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transaction simulation (dry-run / dev-inspect) for simulacrum.
//!
//! TODO: This module is a copy of `AuthorityState::simulate_transaction` (and the private
//! helpers it relies on) from `sui-core`, adapted to simulacrum's store/epoch model — no
//! congestion tracker (`suggested_gas_price` is always `None`), no certificate deny set, and
//! no fullnode-only policy checks. The two implementations need to be merged in a more unified
//! way so that simulacrum and the fullnode share a single simulation code path instead of two
//! copies that must be kept in sync.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use move_core_types::language_storage::TypeTag;
use nonempty::NonEmpty;
use sui_config::{
    transaction_deny_config::TransactionDenyConfig, verifier_signing_config::VerifierSigningConfig,
};
use sui_core::authority::DEV_INSPECT_GAS_COIN_VALUE;
use sui_types::{
    accumulator_root::{AccumulatorKey, AccumulatorObjId, AccumulatorValue},
    balance::Balance,
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    coin_reservation::{CoinReservationResolverTrait, ParsedDigest, ParsedObjectRefWithdrawal},
    digests::{ChainIdentifier, TransactionDigest},
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::{SuiErrorKind, SuiResult, UserInputError, UserInputResult},
    execution_params::{ExecutionOrEarlyError, FundsWithdrawStatus, get_early_execution_error},
    execution_status::ExecutionErrorKind,
    full_checkpoint_content::ObjectSet,
    gas::SuiGasStatus,
    object::{MoveObject, OBJECT_START_VERSION, Object, Owner},
    storage::{ObjectKey, RuntimeObjectResolver, TrackingBackingStore},
    sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait,
    transaction::{
        CallArg, FundsWithdrawalArg, ObjectArg, ObjectReadResult, ProgrammableTransaction,
        TransactionData, TransactionDataAPI, TransactionKind, TxValidityCheckContext,
        get_gasless_allowed_token_types,
    },
    transaction_executor::{SimulateTransactionResult, TransactionChecks},
};

use super::EpochState;
use crate::SimulatorStore;

impl EpochState {
    /// Simulate a transaction against the current epoch state without committing any writes to
    /// the store or enqueueing anything for the next checkpoint. This mirrors the fullnode's
    /// dry-run / dev-inspect semantics: `TransactionChecks::Enabled` matches dry-run,
    /// `TransactionChecks::Disabled` matches dev-inspect, and `allow_mock_gas_coin` injects an
    /// `ObjectID::MAX` gas coin when the transaction has no gas payment.
    ///
    /// See the module-level TODO about unifying this with `sui-core`.
    pub fn simulate_transaction(
        &self,
        store: &dyn SimulatorStore,
        deny_config: &TransactionDenyConfig,
        verifier_signing_config: &VerifierSigningConfig,
        mut transaction: TransactionData,
        checks: TransactionChecks,
        allow_mock_gas_coin: bool,
    ) -> SuiResult<SimulateTransactionResult> {
        if transaction.kind().is_system_tx() {
            return Err(SuiErrorKind::UnsupportedFeatureError {
                error: "simulate does not support system transactions".to_string(),
            }
            .into());
        }

        let protocol_config = &self.protocol_config;
        let dev_inspect = checks.disabled();

        // Reject coin reservations in gas payment when the execution engine
        // doesn't support them.
        if !protocol_config.enable_coin_reservation_obj_refs()
            && transaction
                .gas()
                .iter()
                .any(|obj_ref| ParsedDigest::is_coin_reservation_digest(&obj_ref.2))
        {
            return Err(SuiErrorKind::UnsupportedFeatureError {
                error:
                    "coin reservations in gas payment are not supported at this protocol version"
                        .to_string(),
            }
            .into());
        }

        // Compute input/receiving object kinds before mock gas injection so the mock
        // gas reference is not included in input_object_kinds (it is added to
        // input_objects directly after object loading).
        let input_object_kinds = transaction.input_objects()?;
        let receiving_object_refs = transaction.receiving_objects();

        // Inject mock gas coin before validity_check so that on protocol versions
        // where address-balance gas payments are not yet enabled, the non-empty
        // payment check in validity_check passes for simulate/dev-inspect requests
        // submitted without explicit gas.
        // Also required before the funds-withdrawal processing below so that it sees
        // non-empty payment and doesn't create an address-balance withdrawal for gas.
        // Skip mock gas for gasless transactions — they don't use gas coins.
        let is_gasless = protocol_config.enable_gasless() && transaction.is_gasless_transaction();
        let mock_gas_object = if allow_mock_gas_coin && transaction.gas().is_empty() && !is_gasless
        {
            let obj = Object::new_move(
                MoveObject::new_gas_coin(
                    OBJECT_START_VERSION,
                    ObjectID::MAX,
                    DEV_INSPECT_GAS_COIN_VALUE,
                ),
                Owner::AddressOwner(transaction.gas_data().owner),
                TransactionDigest::genesis_marker(),
            );
            transaction.gas_data_mut().payment = vec![obj.compute_object_reference()];
            Some(obj)
        } else {
            None
        };

        // Full validity check including gas budget and price.
        transaction.validity_check(&TxValidityCheckContext {
            config: protocol_config,
            epoch: self.epoch(),
            chain_identifier: self.chain_identifier,
            reference_gas_price: self.reference_gas_price(),
        })?;

        // Deny checks and funds-withdrawal processing; mirrors
        // `AuthorityState::pre_object_load_checks`.
        sui_transaction_checks::deny::check_transaction_for_signing(
            &transaction,
            &[],
            &input_object_kinds,
            &receiving_object_refs,
            deny_config,
            &store,
        )?;

        let coin_reservation_resolver = StoreCoinReservationResolver { resolver: store };
        let declared_withdrawals = transaction.process_funds_withdrawals_for_signing(
            self.chain_identifier,
            &coin_reservation_resolver,
        )?;
        check_amounts_available(store, &declared_withdrawals)?;
        if protocol_config.gasless_verify_remaining_balance()
            && transaction.is_gasless_transaction()
        {
            let min_amounts = get_gasless_allowed_token_types(protocol_config);
            check_remaining_amounts_after_withdrawal(store, &declared_withdrawals, &min_amounts)?;
        }
        let address_funds: BTreeSet<_> = declared_withdrawals.keys().cloned().collect();

        let tx_digest = transaction.digest();
        let (mut input_objects, receiving_objects) = store.read_objects_for_synchronous_execution(
            &tx_digest,
            &input_object_kinds,
            &receiving_object_refs,
        )?;

        // Add mock gas to input objects after loading (it doesn't exist in the store).
        let mock_gas_id = mock_gas_object.map(|obj| {
            let id = obj.id();
            input_objects.push(ObjectReadResult::new_from_gas_object(&obj));
            id
        });

        let (gas_status, checked_input_objects) = if dev_inspect {
            sui_transaction_checks::check_dev_inspect_input(
                protocol_config,
                &transaction,
                input_objects,
                receiving_objects,
                self.reference_gas_price(),
            )?
        } else {
            sui_transaction_checks::check_transaction_input(
                protocol_config,
                self.reference_gas_price(),
                &transaction,
                input_objects,
                &receiving_objects,
                &self.bytecode_verifier_metrics,
                verifier_signing_config,
            )?
        };

        let (mut kind, signer, gas_data) = transaction.execution_parts();
        let rewritten_inputs = rewrite_transaction_for_coin_reservations(
            self.chain_identifier,
            &coin_reservation_resolver,
            signer,
            &mut kind,
            None,
        )?;
        // Simulacrum has no certificate deny config; use an empty deny set.
        let early_execution_error = get_early_execution_error(
            &tx_digest,
            &checked_input_objects,
            &HashSet::new(),
            &FundsWithdrawStatus::MaybeSufficient,
        );
        // Dev-inspect/simulation path (not committed): no assigned accumulator version here, so
        // the IFFW short-circuit applies unconditionally (`None`), matching non-mainnet
        // execution.
        let execution_params = match early_execution_error {
            None => ExecutionOrEarlyError::ok(None),
            Some(errors) => ExecutionOrEarlyError::failed(errors, None),
        };

        let tracking_store = TrackingBackingStore::new(store.backing_store());

        // Clone inputs for potential retry if object funds check fails post-execution.
        let cloned_input_objects = checked_input_objects.clone();
        let cloned_gas = gas_data.clone();
        let cloned_kind = kind.clone();
        let epoch_id = self.epoch();
        let epoch_timestamp_ms = self.epoch_start_state.epoch_start_timestamp_ms();
        let (inner_temp_store, _, effects, execution_result) =
            self.executor.dev_inspect_transaction(
                &tracking_store,
                protocol_config,
                self.execution_metrics.clone(),
                false, // expensive_checks
                execution_params,
                &epoch_id,
                epoch_timestamp_ms,
                checked_input_objects,
                gas_data,
                gas_status,
                kind,
                rewritten_inputs.clone(),
                signer,
                tx_digest,
                dev_inspect,
            );

        // Post-execution: check object funds (non-address withdrawals discovered during
        // execution).
        let (inner_temp_store, effects, execution_result) = if execution_result.is_ok() {
            let has_insufficient_object_funds = inner_temp_store
                .accumulator_running_max_withdraws
                .iter()
                .filter(|(id, _)| !address_funds.contains(id))
                .any(|(id, max_withdraw)| get_latest_account_amount(store, id) < *max_withdraw);

            if has_insufficient_object_funds {
                let retry_gas_status = SuiGasStatus::new(
                    cloned_gas.budget,
                    cloned_gas.price,
                    self.reference_gas_price(),
                    protocol_config,
                )?;
                let (store, _, effects, result) = self.executor.dev_inspect_transaction(
                    &tracking_store,
                    protocol_config,
                    self.execution_metrics.clone(),
                    false,
                    ExecutionOrEarlyError::failed(
                        NonEmpty::new(ExecutionErrorKind::InsufficientFundsForWithdraw),
                        None,
                    ),
                    &epoch_id,
                    epoch_timestamp_ms,
                    cloned_input_objects,
                    cloned_gas,
                    retry_gas_status,
                    cloned_kind,
                    rewritten_inputs,
                    signer,
                    tx_digest,
                    dev_inspect,
                );
                (store, effects, result)
            } else {
                (inner_temp_store, effects, execution_result)
            }
        } else {
            (inner_temp_store, effects, execution_result)
        };

        let loaded_runtime_objects = tracking_store.into_read_objects();
        let unchanged_loaded_runtime_objects =
            unchanged_loaded_runtime_objects(&transaction, &effects, &loaded_runtime_objects);

        let object_set = {
            let objects = {
                let mut objects = loaded_runtime_objects;

                for o in inner_temp_store
                    .input_objects
                    .into_values()
                    .chain(inner_temp_store.written.into_values())
                {
                    objects.insert(o);
                }

                objects
            };

            let object_keys = sui_types::storage::get_transaction_object_set(
                &transaction,
                &effects,
                &unchanged_loaded_runtime_objects,
            );

            let mut set = ObjectSet::default();
            for k in object_keys {
                if let Some(o) = objects.get(&k) {
                    set.insert(o.clone());
                }
            }

            set
        };

        Ok(SimulateTransactionResult {
            objects: object_set,
            events: effects.events_digest().map(|_| inner_temp_store.events),
            effects,
            execution_result,
            mock_gas_id,
            unchanged_loaded_runtime_objects,
            // Simulacrum has no congestion tracker to suggest gas prices from.
            suggested_gas_price: None,
        })
    }
}

/// Resolves coin reservations against the simulator store.
///
/// Copied from `sui_types::coin_reservation::CoinReservationResolver`, which requires an
/// `Arc<dyn RuntimeObjectResolver>` that a borrowed simulator store cannot provide.
struct StoreCoinReservationResolver<'a> {
    resolver: &'a dyn RuntimeObjectResolver,
}

impl CoinReservationResolverTrait for StoreCoinReservationResolver<'_> {
    fn resolve_funds_withdrawal(
        &self,
        sender: SuiAddress,
        coin_reservation: ParsedObjectRefWithdrawal,
        accumulator_version: Option<SequenceNumber>,
    ) -> UserInputResult<FundsWithdrawalArg> {
        let invalid = |error: String| UserInputError::InvalidWithdrawReservation { error };

        let object = AccumulatorValue::load_object_by_id(
            self.resolver,
            accumulator_version,
            coin_reservation.unmasked_object_id,
        )
        .map_err(|e| invalid(format!("could not load coin reservation object id {e}")))?
        .ok_or_else(|| {
            invalid(format!(
                "coin reservation object id {} not found",
                coin_reservation.unmasked_object_id
            ))
        })?;

        let move_object = object.data.try_as_move().unwrap();

        let type_tag: TypeTag = move_object
            .type_()
            .balance_accumulator_field_type_maybe()
            .ok_or_else(|| {
                invalid(format!(
                    "coin reservation object id {} is not a balance accumulator field",
                    coin_reservation.unmasked_object_id
                ))
            })?;

        let (key, _): (AccumulatorKey, AccumulatorValue) = move_object
            .try_into()
            .map_err(|e| invalid(format!("could not load coin reservation object id {e}")))?;

        if sender != key.owner {
            return Err(invalid(format!(
                "coin reservation object id {} is owned by {}, not sender {}",
                coin_reservation.unmasked_object_id, key.owner, sender
            )));
        }

        Ok(FundsWithdrawalArg::balance_from_sender(
            coin_reservation.parsed_digest.reservation_amount(),
            type_tag,
        ))
    }
}

/// Reads the latest balance of an accumulator (address-balance) account object from the store.
/// Mirrors `AccountFundsRead::get_latest_account_amount` in `sui-core`; a missing account object
/// means a zero balance.
fn get_latest_account_amount(store: &dyn SimulatorStore, account_id: &AccumulatorObjId) -> u128 {
    SimulatorStore::get_object(store, account_id.inner())
        .map(|account_obj| {
            let (_, AccumulatorValue::U128(value)) =
                account_obj.data.try_as_move().unwrap().try_into().unwrap();
            value.value
        })
        .unwrap_or(0)
}

/// Copied from `AccountFundsRead::check_amounts_available` in `sui-core`, backed by the
/// simulator store instead of the fullnode's execution cache.
fn check_amounts_available(
    store: &dyn SimulatorStore,
    requested_amounts: &BTreeMap<AccumulatorObjId, (u64, TypeTag, SuiAddress)>,
) -> SuiResult {
    for (object_id, (requested_amount, type_tag, owner)) in requested_amounts {
        let actual_amount = get_latest_account_amount(store, object_id);

        if actual_amount < *requested_amount as u128 {
            let coin_type =
                Balance::maybe_get_balance_type_param(type_tag).unwrap_or_else(|| type_tag.clone());
            return Err(SuiErrorKind::UserInputError {
                error: UserInputError::InvalidWithdrawReservation {
                    error: format!(
                        "Insufficient address balance of coin type {coin_type} \
                         for address {owner}: the transaction requires \
                         {requested_amount} but only {actual_amount} is available. \
                         Note that the address balance does not include funds held \
                         in Coin objects owned by the address; to spend those funds, \
                         use the Coin objects directly as transaction inputs.",
                    ),
                },
            }
            .into());
        }
    }

    Ok(())
}

/// Copied from `AccountFundsRead::check_remaining_amounts_after_withdrawal` in `sui-core`,
/// backed by the simulator store instead of the fullnode's execution cache.
fn check_remaining_amounts_after_withdrawal(
    store: &dyn SimulatorStore,
    requested_amounts: &BTreeMap<AccumulatorObjId, (u64, TypeTag, SuiAddress)>,
    min_amounts: &BTreeMap<TypeTag, u64>,
) -> SuiResult {
    for (object_id, (requested_amount, type_tag, owner)) in requested_amounts {
        let actual_amount = get_latest_account_amount(store, object_id);
        let remaining = actual_amount.saturating_sub(*requested_amount as u128);
        if remaining == 0 {
            continue;
        }
        let coin_type =
            Balance::maybe_get_balance_type_param(type_tag).unwrap_or_else(|| type_tag.clone());
        if let Some(&min_amount) = min_amounts.get(&coin_type)
            && min_amount > 0
            && remaining < min_amount as u128
        {
            return Err(SuiErrorKind::UserInputError {
                error: UserInputError::InvalidWithdrawReservation {
                    error: format!(
                        "Invalid gasless withdrawal of coin type {coin_type} \
                         from address {owner}. \
                         Gasless transactions must either use the entire address \
                         balance, or leave at least {min_amount}. \
                         Remaining amount would be {remaining}",
                    ),
                },
            }
            .into());
        }
    }

    Ok(())
}

/// Rewrites coin reservation inputs (fake coins encoded as masked ObjectRefs) into
/// FundsWithdrawalArgs so the executor can resolve them as balance withdrawals.
///
/// Copied from `sui-core`'s private `accumulators::transaction_rewriting` module.
fn rewrite_transaction_for_coin_reservations(
    chain_identifier: ChainIdentifier,
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    sender: SuiAddress,
    transaction_kind: &mut TransactionKind,
    accumulator_version: Option<SequenceNumber>,
) -> UserInputResult<Option<Vec<bool>>> {
    match transaction_kind {
        TransactionKind::ProgrammableTransaction(pt) => {
            rewrite_programmable_transaction_for_coin_reservations(
                chain_identifier,
                coin_reservation_resolver,
                sender,
                pt,
                accumulator_version,
            )
        }
        _ => Ok(None),
    }
}

fn rewrite_programmable_transaction_for_coin_reservations(
    chain_identifier: ChainIdentifier,
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    sender: SuiAddress,
    pt: &mut ProgrammableTransaction,
    accumulator_version: Option<SequenceNumber>,
) -> UserInputResult<Option<Vec<bool>>> {
    if pt.coin_reservation_obj_refs().count() == 0 {
        return Ok(None);
    }

    let mut rewritten_inputs = Vec::with_capacity(pt.inputs.len());
    for input in pt.inputs.iter_mut() {
        if let CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)) = input
            && let Some(parsed) = ParsedObjectRefWithdrawal::parse(object_ref, chain_identifier)
        {
            rewritten_inputs.push(true);

            let withdraw = coin_reservation_resolver.resolve_funds_withdrawal(
                sender,
                parsed,
                accumulator_version,
            )?;
            *input = CallArg::FundsWithdrawal(withdraw);
        } else {
            rewritten_inputs.push(false);
        }
    }

    Ok(Some(rewritten_inputs))
}

/// Copied from `sui-core`'s private `transaction_outputs::unchanged_loaded_runtime_objects`.
fn unchanged_loaded_runtime_objects(
    _transaction: &TransactionData,
    effects: &TransactionEffects,
    loaded_runtime_objects: &ObjectSet,
) -> Vec<ObjectKey> {
    let mut unchanged_loaded_runtime_objects: BTreeMap<_, _> = loaded_runtime_objects
        .iter()
        // Don't include loaded packages (which are used for doing UID tracking inside the VM)
        .filter(|o| !o.is_package())
        .map(|o| (o.id(), o.version()))
        .collect();

    // Remove any object that is referenced in the changed objects effects set since it would be
    // redundant to include it again.
    for change in effects.object_changes() {
        unchanged_loaded_runtime_objects.remove(&change.id);
    }

    unchanged_loaded_runtime_objects
        .into_iter()
        .map(|(id, v)| ObjectKey(id, v))
        .collect()
}
