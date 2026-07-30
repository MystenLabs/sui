// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use itertools::Itertools;
use sui_protocol_config::ProtocolConfig;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Bcs;
use sui_rpc::proto::sui::rpc::v2::CommandOutput;
use sui_rpc::proto::sui::rpc::v2::CommandResult;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::ObjectSet;
use sui_rpc::proto::sui::rpc::v2::SimulateTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::SimulateTransactionResponse;
use sui_rpc::proto::sui::rpc::v2::Transaction;
use sui_types::balance_change::derive_balance_changes_2;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::SuiError;
use sui_types::error::SuiErrorKind;
use sui_types::execution_status::ExecutionFailure;
use sui_types::execution_status::ExecutionStatus;
use sui_types::transaction::InputObjectKind;
use sui_types::transaction::InputObjects;
use sui_types::transaction::ObjectReadResult;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction::TransactionExpiration;
use sui_types::transaction::TransactionKind;
use sui_types::transaction_executor::SimulateTransactionResult;
use sui_types::transaction_executor::TransactionChecks;

mod resolve;

pub fn simulate_transaction(
    service: &RpcService,
    request: SimulateTransactionRequest,
) -> Result<SimulateTransactionResponse> {
    let executor = service
        .executor
        .as_ref()
        .ok_or_else(|| RpcError::new(tonic::Code::Unimplemented, "no transaction executor"))?;

    let read_mask = request
        .read_mask
        .as_ref()
        .map(FieldMaskTree::from_field_mask)
        .unwrap_or_else(FieldMaskTree::new_wildcard);

    let transaction_proto = request
        .transaction
        .as_ref()
        .ok_or_else(|| FieldViolation::new("transaction").with_reason(ErrorReason::FieldMissing))?;

    let checks = TransactionChecks::from(request.checks());

    // TODO make this more efficient
    let (reference_gas_price, protocol_config) = {
        let system_state = service.reader.get_system_state_summary()?;
        let protocol_config = ProtocolConfig::get_for_version_if_supported(
            system_state.protocol_version.into(),
            service.reader.inner().get_chain_identifier()?.chain(),
        )
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                "unable to get current protocol config",
            )
        })?;

        (system_state.reference_gas_price, protocol_config)
    };

    // Try to parse out a fully-formed transaction. If one wasn't provided then we will attempt to
    // perform transaction resolution.
    let mut transaction = match sui_sdk_types::Transaction::try_from(transaction_proto) {
        Ok(transaction) => sui_types::transaction::TransactionData::try_from(transaction)?,

        // If we weren't able to parse out a fully-formed transaction and the client provided BCS
        // TransactionData, then we'll error out early since we're unable to perform resolution
        // given a BCS payload
        Err(e) if transaction_proto.bcs.is_some() => {
            return Err(FieldViolation::new("transaction")
                .with_description(format!("invalid transaction: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }

        // We weren't able to parse out a fully-formed transaction so we'll attempt to perform
        // transaction resolution
        _ => resolve::resolve_transaction(
            service,
            transaction_proto,
            reference_gas_price,
            &protocol_config,
        )?,
    };

    let perform_gas_selection = request.do_gas_selection() && checks.enabled();
    let skip_gas_selection_for_bcs_gasless = perform_gas_selection
        && request.transaction().bcs_opt().is_some()
        && transaction.is_gasless_transaction();

    // Without automatic selection (or for a BCS gasless request, which deliberately skips it),
    // the empty payment remains in place. Normalize it here as well as in the authority so that
    // the transaction returned to the caller is the one simulated. With selection, `select_gas`
    // adds this only when it chooses address balance.
    if (!perform_gas_selection || skip_gas_selection_for_bcs_gasless)
        && protocol_config.enable_accumulators()
        && protocol_config.enable_address_balance_gas_payments()
        && transaction.is_gas_paid_from_address_balance()
        && matches!(transaction.expiration(), TransactionExpiration::None)
    {
        set_valid_during_transaction_expiration(service, &mut transaction)?;
    }

    let simulation_result = 'simulate: {
        // BCS transactions that are already in gasless shape (price=0, no payment) are simulated
        // as-is — the caller pre-built the transaction, so gas selection is not needed and the
        // priced flow must be skipped (it would fail with GasPriceUnderRGP for price=0).
        if perform_gas_selection && !skip_gas_selection_for_bcs_gasless {
            // If the caller didn't set a non-zero price and the tx passes the cheap structural +
            // object-input gasless checks, try a gasless simulate first. Post-execution gasless
            // requirements (all input Coins consumed, minimum transfer amounts) can only be
            // verified by running the tx. If that fails, we discard the gasless variant and
            // fall through to the priced flow. `payment` is already empty here, verified by
            // is_gasless_candidate.
            if is_gasless_candidate(&request, &transaction, &protocol_config, service)? {
                let mut gasless_tx = transaction.clone();
                gasless_tx.gas_data_mut().price = 0;
                gasless_tx.gas_data_mut().budget = 0;
                // All gassless txns have to have a correct `ValidDuring` TransactionExpiration.
                set_valid_during_transaction_expiration(service, &mut gasless_tx)?;

                let simulation_result = executor
                    .simulate_transaction(gasless_tx.clone(), checks)
                    .map_err(simulation_error_to_rpc_error)?;

                if !is_gasless_post_execution_failure(simulation_result.effects.status()) {
                    transaction = gasless_tx;
                    break 'simulate simulation_result;
                }
            }

            // Priced-flow budget estimation and gas selection.
            // At this point, the budget on the transaction can be set to one of the following:
            // - The budget from the request, if specified.
            // - The total balance of all of the gas payment coins (clamped to the protocol
            //   MAX_GAS_BUDGET) in the request if the budget was not
            //   specified but the gas payment coins were specified.
            // - Protocol MAX_GAS_BUDGET if the request did not specified neither gas payment or budget.
            //
            // If the request did not specify a budget, then simulate the transaction to get a budget estimate and
            // overwrite the resolved budget with the more accurate estimate.
            // When the request didn't specify a budget, the budget computed below covers
            // computation + storage + safe-overhead. The estimation transaction uses only a
            // genuine payment source: supplied gas coins, address balance, or selected gas coins.
            let budget_was_estimated = request.transaction().gas_payment().budget.is_none()
                && request.transaction().bcs_opt().is_none();
            if budget_was_estimated {
                let simulation_result = if transaction.gas_data().payment.is_empty() {
                    estimate_with_real_gas_payment(
                        service,
                        executor.as_ref(),
                        &transaction,
                        &protocol_config,
                    )?
                } else {
                    executor
                        .simulate_transaction(transaction.clone(), TransactionChecks::Enabled)
                        .map_err(simulation_error_to_rpc_error)?
                };

                let estimate = estimate_gas_budget_from_gas_cost(
                    simulation_result.effects.gas_cost_summary(),
                    reference_gas_price,
                    &protocol_config,
                );

                // If the request specified gas payment, then transaction.gas_data().budget should have been
                // resolved to the cumulative balance of those coins. We don't want to return a resolved transaction
                // where the gas payment can't satisfy the budget, so validate that balance can actually cover the
                // estimated budget.
                let gas_balance = transaction.gas_data().budget;
                if gas_balance < estimate {
                    return Err(RpcError::new(
                        tonic::Code::InvalidArgument,
                        format!(
                            "Insufficient gas balance to cover estimated transaction cost. \
                            Available gas balance: {gas_balance} MIST. Estimated gas budget required: {estimate} MIST"
                        ),
                    ));
                }
                transaction.gas_data_mut().budget = estimate;
            }

            if transaction.gas_data().payment.is_empty() {
                select_gas(service, &mut transaction, true, &protocol_config)?;
            }
        }

        executor
            .simulate_transaction(transaction.clone(), checks)
            .map_err(simulation_error_to_rpc_error)?
    };

    let SimulateTransactionResult {
        effects,
        events,
        objects,
        execution_result,
        unchanged_loaded_runtime_objects,
        suggested_gas_price,
    } = simulation_result;

    let transaction = if let Some(submask) = read_mask.subtree("transaction") {
        let mut message = ExecutedTransaction::default();
        let transaction = sui_sdk_types::Transaction::try_from(transaction)?;

        message.balance_changes =
            if submask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name) {
                derive_balance_changes_2(&effects, &objects)
                    .into_iter()
                    .map(Into::into)
                    .collect()
            } else {
                vec![]
            };

        message.effects = submask
            .subtree(ExecutedTransaction::EFFECTS_FIELD)
            .map(|mask| {
                service.render_effects_to_proto(
                    &effects,
                    &unchanged_loaded_runtime_objects,
                    &objects,
                    &mask,
                )
            });

        message.events = submask
            .subtree(ExecutedTransaction::EVENTS_FIELD.name)
            .and_then(|mask| {
                events.map(|events| service.render_events_to_proto(&events, &mask, &objects))
            });

        message.transaction = submask
            .subtree(ExecutedTransaction::TRANSACTION_FIELD.name)
            .map(|mask| Transaction::merge_from(transaction, &mask));

        message.objects = submask
            .subtree(
                ExecutedTransaction::path_builder()
                    .objects()
                    .objects()
                    .finish(),
            )
            .map(|mask| {
                ObjectSet::default().with_objects(
                    objects
                        .iter()
                        .map(|o| service.render_object_to_proto(o, &mask, &objects))
                        .collect(),
                )
            });

        Some(message)
    } else {
        None
    };

    let outputs = if read_mask.contains(SimulateTransactionResponse::COMMAND_OUTPUTS_FIELD) {
        execution_result
            .into_iter()
            .flatten()
            .map(|(reference_outputs, return_values)| {
                let mut message = CommandResult::default();
                message.return_values = return_values
                    .into_iter()
                    .map(|(bcs, ty)| to_command_output(service, None, bcs, ty))
                    .collect();
                message.mutated_by_ref = reference_outputs
                    .into_iter()
                    .map(|(arg, bcs, ty)| to_command_output(service, Some(arg), bcs, ty))
                    .collect();
                message
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut response = SimulateTransactionResponse::default();
    response.transaction = transaction;
    response.command_outputs = outputs;
    if read_mask.contains(SimulateTransactionResponse::SUGGESTED_GAS_PRICE_FIELD) {
        response.suggested_gas_price = suggested_gas_price;
    }
    Ok(response)
}

fn simulation_error_to_rpc_error(error: SuiError) -> RpcError {
    match error.as_inner() {
        SuiErrorKind::UserInputError { .. } => {
            RpcError::new(tonic::Code::InvalidArgument, error.to_string())
        }
        SuiErrorKind::UnsupportedFeatureError { .. } => {
            RpcError::new(tonic::Code::InvalidArgument, error.to_string())
        }
        _ => RpcError::new(tonic::Code::Internal, error.to_string()),
    }
}

fn to_command_output(
    service: &RpcService,
    arg: Option<sui_types::transaction::Argument>,
    bcs: Vec<u8>,
    ty: sui_types::TypeTag,
) -> CommandOutput {
    let json = service
        .reader
        .inner()
        .get_type_layout(&ty)
        .ok()
        .flatten()
        .and_then(|layout| {
            let bound = service.config.max_json_move_value_size();
            sui_types::object::rpc_visitor::proto::ProtoVisitor::new(bound)
                .deserialize_value(&bcs, &layout)
                .map_err(|e| tracing::debug!("unable to convert to JSON: {e}"))
                .ok()
                .map(Box::new)
        });

    let mut message = CommandOutput::default();
    message.argument = arg.map(Into::into);
    message.value = Some(Bcs::from(bcs).with_name(ty.to_canonical_string(true)));
    message.json = json;
    message
}

/// Estimate the gas budget for a transaction based on simulation results.
///
/// The estimation includes the actual simulation cost (computation and storage), rounded up to
/// the protocol gas step with a safe overhead buffer, then clamped to the protocol maximum.
fn estimate_gas_budget_from_gas_cost(
    gas_cost_summary: &sui_types::gas::GasCostSummary,
    reference_gas_price: u64,
    protocol_config: &ProtocolConfig,
) -> u64 {
    const GAS_SAFE_OVERHEAD: u64 = 1000;

    let gas_used = gas_cost_summary
        .computation_cost
        .saturating_add(gas_cost_summary.storage_cost);
    let net_gas_usage = (gas_used as i64).saturating_sub(gas_cost_summary.storage_rebate as i64);
    let base_estimate_mist = gas_cost_summary.computation_cost.max(if net_gas_usage < 0 {
        0
    } else {
        net_gas_usage as u64
    });

    let safe_overhead_mist = GAS_SAFE_OVERHEAD.saturating_mul(reference_gas_price);

    base_estimate_mist
        .saturating_add(safe_overhead_mist)
        .min(protocol_config.max_tx_gas())
}

/// Simulate an unresolved, empty-payment transaction using real available funds. Address balance
/// is attempted first; if its full available balance cannot cover execution, retry using owned gas
/// coins. Both probes use real funds and never use a synthetic payment object.
fn estimate_with_real_gas_payment(
    service: &RpcService,
    executor: &dyn sui_types::transaction_executor::TransactionExecutor,
    transaction: &sui_types::transaction::TransactionData,
    protocol_config: &ProtocolConfig,
) -> Result<SimulateTransactionResult> {
    let can_use_address_balance = protocol_config.enable_accumulators()
        && protocol_config.enable_address_balance_gas_payments()
        && !transaction
            .kind()
            .iter_commands()
            .any(sui_types::transaction::Command::is_gas_coin_used);

    if can_use_address_balance
        && let Some(address_balance) = available_address_balance(service, transaction)
        && address_balance > 0
    {
        let mut address_balance_transaction = transaction.clone();
        address_balance_transaction.gas_data_mut().budget =
            address_balance.min(protocol_config.max_tx_gas());
        if matches!(
            address_balance_transaction.expiration(),
            TransactionExpiration::None
        ) {
            set_valid_during_transaction_expiration(service, &mut address_balance_transaction)?;
        }

        match executor.simulate_transaction(address_balance_transaction, TransactionChecks::Enabled)
        {
            Ok(result) if !is_insufficient_gas(&result) => return Ok(result),
            Ok(_) => {}
            Err(error) if is_gas_budget_too_low(&error) => {}
            Err(error) => return Err(simulation_error_to_rpc_error(error)),
        }
    }

    let mut coin_transaction = transaction.clone();
    // Select all eligible real gas coins first, then simulate using the amount they can actually
    // cover. A temporary budget of one only drives selection; it is never sent to the executor.
    coin_transaction.gas_data_mut().budget = 1;
    let available = select_gas(service, &mut coin_transaction, false, protocol_config)?;
    coin_transaction.gas_data_mut().budget = available.min(protocol_config.max_tx_gas());

    executor
        .simulate_transaction(coin_transaction, TransactionChecks::Enabled)
        .map_err(simulation_error_to_rpc_error)
}

fn is_insufficient_gas(simulation_result: &SimulateTransactionResult) -> bool {
    matches!(
        simulation_result.effects.status(),
        ExecutionStatus::Failure(ExecutionFailure {
            error: sui_types::execution_status::ExecutionErrorKind::InsufficientGas,
            ..
        })
    )
}

fn is_gas_budget_too_low(error: &SuiError) -> bool {
    matches!(
        error.as_inner(),
        SuiErrorKind::UserInputError {
            error: sui_types::error::UserInputError::GasBudgetTooLow { .. },
        }
    )
}

/// Populate a `ValidDuring` expiration covering the current epoch and the next one.
fn set_valid_during_transaction_expiration(
    service: &RpcService,
    transaction: &mut sui_types::transaction::TransactionData,
) -> Result<()> {
    // Early return if the TransactionExpiration is already set to `ValidDuring`
    if matches!(
        transaction.expiration(),
        TransactionExpiration::ValidDuring { .. }
    ) {
        return Ok(());
    }

    let current_epoch = service.reader.inner().get_latest_checkpoint()?.epoch();
    *transaction.expiration_mut() = TransactionExpiration::ValidDuring {
        min_epoch: Some(current_epoch),
        max_epoch: Some(current_epoch.saturating_add(1)),
        min_timestamp: None,
        max_timestamp: None,
        chain: service.chain_id,
        nonce: rand::random(),
    };
    Ok(())
}

fn available_address_balance(
    service: &RpcService,
    transaction: &sui_types::transaction::TransactionData,
) -> Option<u64> {
    use sui_types::accumulator_root::AccumulatorValue;
    use sui_types::balance::Balance;
    use sui_types::coin_reservation::CoinReservationResolver;
    use sui_types::gas_coin::GAS;

    let owner = transaction.gas_data().owner;
    service
        .reader
        .lookup_address_balance(owner, GAS::type_())
        .map(|balance| {
            // Exclude explicit SUI reservations, but not the implicit gas reservation: this
            // helper is used to determine the budget that the gas payment itself may consume.
            let coin_resolver = CoinReservationResolver::new(service.reader.inner().clone());
            let reserved_sui = transaction
                .process_funds_withdrawals_for_estimation(service.chain_id, &coin_resolver)
                .ok()
                .and_then(|withdrawals| {
                    let sui_type = Balance::type_tag(GAS::type_tag());
                    let sui_account_id = AccumulatorValue::get_field_id(owner, &sui_type).ok()?;
                    withdrawals
                        .get(&sui_account_id)
                        .map(|(amount, _, _)| *amount)
                })
                .unwrap_or(0);
            balance.saturating_sub(reserved_sui)
        })
}

fn select_gas(
    service: &RpcService,
    transaction: &mut sui_types::transaction::TransactionData,
    prefer_address_balance: bool,
    protocol_config: &ProtocolConfig,
) -> Result<u64> {
    use sui_types::accumulator_root::AccumulatorValue;
    use sui_types::balance::Balance;
    use sui_types::base_types::SequenceNumber;
    use sui_types::coin_reservation::ParsedObjectRefWithdrawal;
    use sui_types::gas_coin::GAS;
    use sui_types::gas_coin::GasCoin;
    use sui_types::transaction::Command;
    use sui_types::transaction::TransactionDataAPI;

    let reader = &service.reader;

    let owner = transaction.gas_data().owner;
    let budget = transaction.gas_data().budget;

    let gas_coin_used = transaction
        .kind()
        .iter_commands()
        .any(Command::is_gas_coin_used);
    let address_balance = available_address_balance(service, transaction);

    // If the gas coin isn't used and there is sufficient address balance budget to satisfy the
    // required budget then we will use the `owner`s address balance to pay for gas. Otherwise we
    // fallback to doing coin selection
    let selected_gas_value = if prefer_address_balance
        && protocol_config.enable_accumulators()
        && protocol_config.enable_address_balance_gas_payments()
        && !gas_coin_used
        && let Some(address_balance) = address_balance
        && address_balance >= budget
    {
        // We probably don't need to do this, but explicitly clear out the payment to force using
        // Address balance
        transaction.gas_data_mut().payment.clear();

        if matches!(transaction.expiration(), TransactionExpiration::None) {
            set_valid_during_transaction_expiration(service, transaction)?;
        }

        budget
    } else {
        let input_objects = transaction
            .input_objects()
            .map_err(anyhow::Error::from)?
            .iter()
            .flat_map(|obj| match obj {
                sui_types::transaction::InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => {
                    Some(*id)
                }
                _ => None,
            })
            .collect_vec();

        let gas_coins = reader
            .inner()
            .indexes()
            .ok_or_else(RpcError::not_found)?
            .owned_objects_iter(owner, Some(GasCoin::type_()), None)?
            .filter_ok(|info| !input_objects.contains(&info.object_id))
            .filter_map_ok(|info| reader.inner().get_object(&info.object_id))
            // filter for objects which are not ConsensusAddress owned,
            // since only Address owned can be used for gas payments today
            .filter_ok(|object| !object.is_consensus())
            .filter_map_ok(|object| {
                GasCoin::try_from(&object)
                    .ok()
                    .map(|coin| (object.compute_object_reference(), coin.value()))
            })
            .take(protocol_config.max_gas_payment_objects() as usize);

        let mut selected_gas = vec![];
        let mut selected_gas_value = 0;

        for maybe_coin in gas_coins {
            let (object_ref, value) =
                maybe_coin.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
            selected_gas.push(object_ref);
            selected_gas_value += value;
        }

        // When GasCoin is used and there's address balance, prepend a coin reservation
        // to make all SUI in the account available (coins + address balance)
        if protocol_config.enable_coin_reservation_obj_refs()
            && gas_coin_used
            && let Some(ab_value) = address_balance
            && ab_value > 0
        {
            let current_epoch = service.reader.inner().get_latest_checkpoint()?.epoch();

            let accumulator_obj_id =
                AccumulatorValue::get_field_id(owner, &Balance::type_tag(GAS::type_tag()))
                    .map_err(|e| {
                        RpcError::new(
                            tonic::Code::Internal,
                            format!("Failed to get accumulator object ID: {e}"),
                        )
                    })?;

            let reservation = ParsedObjectRefWithdrawal::new(
                *accumulator_obj_id.inner(),
                current_epoch,
                ab_value,
            );
            let coin_reservation = reservation.encode(SequenceNumber::new(), service.chain_id);

            // Prepend coin reservation to make address balance accessible via GasCoin
            selected_gas.insert(0, coin_reservation);
            selected_gas_value += ab_value;

            if matches!(transaction.expiration(), TransactionExpiration::None) {
                set_valid_during_transaction_expiration(service, transaction)?;
            }
        }

        transaction.gas_data_mut().payment = selected_gas;

        selected_gas_value
    };

    if selected_gas_value >= budget {
        Ok(selected_gas_value)
    } else {
        Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!(
                "Unable to perform gas selection due to insufficient SUI \
                balance (in address balance or coins) for account {owner} \
                to satisfy required budget {budget}."
            ),
        ))
    }
}

/// Returns true if the simulate request is eligible for auto gas_price=0 handling.
///
/// Requires: gasless enabled by protocol, caller did not set price (or set it to 0) and did not
/// set gas payment objects, tx is a PTB that passes the structural gasless checks, and all loaded
/// Move object inputs pass the runtime gasless input check (`Coin<T>` with `T` allowlisted,
/// AddressOwner/ConsensusAddressOwner).
fn is_gasless_candidate(
    request: &SimulateTransactionRequest,
    transaction: &sui_types::transaction::TransactionData,
    protocol_config: &ProtocolConfig,
    service: &RpcService,
) -> Result<bool> {
    if !protocol_config.enable_gasless() {
        return Ok(false);
    }
    // When the caller passed a full BCS TransactionData, treat it as explicit — don't second-guess
    // their gas choice. Only auto-switch in the unresolved/proto path.
    if request.transaction().bcs_opt().is_some() {
        return Ok(false);
    }
    // An explicit non-zero price opts out of gasless. price=0 is treated the same as unset: the
    // caller is either asserting gasless intent or echoing the suggestion from a prior simulate.
    if request
        .transaction()
        .gas_payment()
        .price
        .is_some_and(|p| p != 0)
    {
        return Ok(false);
    }
    if !request.transaction().gas_payment().objects.is_empty() {
        return Ok(false);
    }
    let TransactionKind::ProgrammableTransaction(pt) = transaction.kind() else {
        return Ok(false);
    };
    if pt.validate_gasless_transaction(protocol_config).is_err() {
        return Ok(false);
    }

    // Load Move object inputs so we can run the runtime input check. Packages need not be loaded
    // since check_gasless_object_inputs skips them.
    let input_object_kinds = match transaction.input_objects() {
        Ok(kinds) => kinds,
        Err(_) => return Ok(false),
    };
    let mut loaded = Vec::with_capacity(input_object_kinds.len());
    for kind in input_object_kinds {
        match kind {
            InputObjectKind::MovePackage(_) => continue,
            InputObjectKind::ImmOrOwnedMoveObject(object_ref) => {
                let Some(object) = service.reader.inner().get_object(&object_ref.0) else {
                    return Ok(false);
                };
                loaded.push(ObjectReadResult::new(kind, object.into()));
            }
            InputObjectKind::SharedMoveObject { id, .. } => {
                let Some(object) = service.reader.inner().get_object(&id) else {
                    return Ok(false);
                };
                loaded.push(ObjectReadResult::new(kind, object.into()));
            }
        }
    }
    let input_objects = InputObjects::new(loaded);
    Ok(
        sui_transaction_checks::check_gasless_object_inputs(&input_objects, protocol_config)
            .is_ok(),
    )
}

/// The executor maps a post-execution gasless-requirements failure
/// (`TemporaryStore::check_gasless_execution_requirements`) to
/// `ExecutionErrorKind::InsufficientGas` on the effects (see
/// `sui-execution/latest/sui-adapter/src/execution_engine.rs`). During a gasless simulate, that's
/// the only way InsufficientGas can surface (gasless uses a large compute cap and ignores budget),
/// so we treat it as the fallback trigger.
fn is_gasless_post_execution_failure(status: &ExecutionStatus) -> bool {
    matches!(
        status,
        ExecutionStatus::Failure(ExecutionFailure {
            error: sui_types::execution_status::ExecutionErrorKind::InsufficientGas,
            ..
        })
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::base_types::ObjectID;
    use sui_types::error::UserInputError;

    #[test]
    fn maps_simulation_user_input_errors_to_invalid_argument() {
        let error = SuiErrorKind::UserInputError {
            error: UserInputError::ObjectNotFound {
                object_id: ObjectID::ZERO,
                version: None,
            },
        }
        .into();

        let status = simulation_error_to_rpc_error(error).into_status_proto();

        assert_eq!(status.code, tonic::Code::InvalidArgument as i32);
        assert!(
            status
                .message
                .contains("Error checking transaction input objects")
        );
    }

    #[test]
    fn maps_simulation_unsupported_feature_errors_to_invalid_argument() {
        let error = SuiErrorKind::UnsupportedFeatureError {
            error: "not supported".to_string(),
        }
        .into();

        let status = simulation_error_to_rpc_error(error).into_status_proto();

        assert_eq!(status.code, tonic::Code::InvalidArgument as i32);
    }

    #[test]
    fn maps_uncategorized_simulation_errors_to_internal() {
        let error = SuiErrorKind::Unknown("boom".to_string()).into();

        let status = simulation_error_to_rpc_error(error).into_status_proto();

        assert_eq!(status.code, tonic::Code::Internal as i32);
    }
}
