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
use sui_types::transaction::TransactionKind;
use sui_types::transaction_executor::SimulateTransactionResult;
use sui_types::transaction_executor::TransactionChecks;

mod resolve;

const GAS_COIN_SIZE_BYTES: u64 = 40;

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
    let simulation_result = 'simulate: {
        if perform_gas_selection {
            // If the caller didn't set a price and the tx passes the cheap structural +
            // object-input gasless checks, try a gasless simulate first. Post-execution gasless
            // requirements (all input Coins consumed, minimum transfer amounts) can only be
            // verified by running the tx. If that fails, we discard the gasless variant and
            // fall through to the priced flow. `payment` is already empty here, verified by
            // is_gasless_candidate.
            if is_gasless_candidate(&request, &transaction, &protocol_config, service)? {
                let mut gasless_tx = transaction.clone();
                gasless_tx.gas_data_mut().price = 0;
                gasless_tx.gas_data_mut().budget = 0;

                let simulation_result = executor
                    .simulate_transaction(gasless_tx.clone(), checks, false)
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
            // computation + storage + safe-overhead, with the synthetic gas coin's storage
            // cost subtracted (it doesn't exist at execution time). The cost of loading
            // any additional payment objects is added either in `estimate_gas_budget_from_gas_cost`
            // (when payment was specified) or incrementally inside `select_gas` (when gas
            // selection picks the coins).
            let budget_was_estimated = request.transaction().gas_payment().budget.is_none()
                && request.transaction().bcs_opt().is_none();
            if budget_was_estimated {
                let mut estimation_transaction = transaction.clone();
                estimation_transaction.gas_data_mut().payment = Vec::new();
                estimation_transaction.gas_data_mut().budget = protocol_config.max_tx_gas();

                let simulation_result = executor
                    .simulate_transaction(
                        estimation_transaction,
                        TransactionChecks::Enabled,
                        true, /* allow mock gas coin */
                    )
                    .map_err(simulation_error_to_rpc_error)?;

                let estimate = estimate_gas_budget_from_gas_cost(
                    simulation_result.effects.gas_cost_summary(),
                    reference_gas_price,
                    request.transaction().gas_payment().objects.len(),
                    mock_gas_storage_cost(&simulation_result),
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
                select_gas(
                    service,
                    &mut transaction,
                    // Only adjust the budget for actually-selected coins when we just
                    // computed the budget from estimation. A caller-supplied budget is
                    // taken as-is.
                    budget_was_estimated.then_some(reference_gas_price),
                    &protocol_config,
                )?;
            }
        }

        executor
            .simulate_transaction(transaction.clone(), checks, !perform_gas_selection)
            .map_err(simulation_error_to_rpc_error)?
    };

    let SimulateTransactionResult {
        effects,
        events,
        objects,
        execution_result,
        mock_gas_id,
        unchanged_loaded_runtime_objects,
        suggested_gas_price,
    } = simulation_result;

    if perform_gas_selection && mock_gas_id.is_some() {
        // If we don't allow for using a mock coin, but we still did, return a server error
        return Err(RpcError::new(
            tonic::Code::Internal,
            "unexpected mock gas coin used",
        ));
    }

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
/// The estimation includes:
/// 1. Base cost from gas_cost_summary (computation + storage costs), with the synthetic gas
///    coin's storage cost subtracted (it doesn't exist at execution time).
/// 2. Cost of loading additional gas payment objects beyond the synthetic gas coin already
///    in the simulation. When the request didn't specify gas payment objects, this is 0 —
///    `select_gas` will pick the actual coins (or address balance) and adjust the budget
///    incrementally for each one.
/// 3. Rounding up to the protocol gas rounding step (typically 1000 MIST).
/// 4. Adding safe overhead buffer (1000 * reference_gas_price).
/// 5. Clamping to max_tx_gas protocol limit.
fn estimate_gas_budget_from_gas_cost(
    gas_cost_summary: &sui_types::gas::GasCostSummary,
    reference_gas_price: u64,
    num_payment_objects_on_request: usize,
    mock_gas_storage_cost: u64,
    protocol_config: &ProtocolConfig,
) -> u64 {
    const GAS_SAFE_OVERHEAD: u64 = 1000;

    // The simulation always loads a synthetic gas coin so that it can produce a gas cost
    // summary even when the caller hasn't specified a gas payment. That coin's storage
    // write is phantom — it is not written at execution time (real address-balance gas
    // emits an accumulator event, real coin gas writes the user-provided coin instead) —
    // so subtract its contribution from `storage_cost` before deriving the estimate.
    let storage_cost = gas_cost_summary
        .storage_cost
        .saturating_sub(mock_gas_storage_cost);
    let gas_used = gas_cost_summary
        .computation_cost
        .saturating_add(storage_cost);
    let net_gas_usage = (gas_used as i64).saturating_sub(gas_cost_summary.storage_rebate as i64);
    let base_estimate_mist = gas_cost_summary.computation_cost.max(if net_gas_usage < 0 {
        0
    } else {
        net_gas_usage as u64
    });

    // Loading cost for additional gas coins beyond the synthetic gas coin already counted
    // by the simulation. When the request did not specify any payment objects, the loading
    // cost is added incrementally inside `select_gas` once the actual coins are known.
    let extra_payment_objects = (num_payment_objects_on_request as u64).saturating_sub(1);
    let gas_loading_cost_mist =
        compute_gas_loading_cost_mist(extra_payment_objects, reference_gas_price, protocol_config);

    let safe_overhead_mist = GAS_SAFE_OVERHEAD.saturating_mul(reference_gas_price);

    base_estimate_mist
        .saturating_add(gas_loading_cost_mist)
        .saturating_add(safe_overhead_mist)
        .min(protocol_config.max_tx_gas())
}

/// Cost in MIST of loading `extra_coins` gas-payment objects beyond the synthetic gas coin
/// already accounted for by the estimation simulation. Mirrors the protocol's per-byte
/// read cost, rounded up to the protocol gas rounding step in gas units before being
/// converted to MIST.
fn compute_gas_loading_cost_mist(
    extra_coins: u64,
    reference_gas_price: u64,
    protocol_config: &ProtocolConfig,
) -> u64 {
    let units = extra_coins
        .saturating_mul(GAS_COIN_SIZE_BYTES)
        .saturating_mul(protocol_config.obj_access_cost_read_per_byte());
    let rounded = if let Some(step) = protocol_config.gas_rounding_step_as_option() {
        round_up_to_nearest(units, step)
    } else {
        units
    };
    rounded.saturating_mul(reference_gas_price)
}

/// Round up a value to the nearest multiple of `step` using saturating arithmetic.
fn round_up_to_nearest(value: u64, step: u64) -> u64 {
    let remainder = value % step;
    if remainder == 0 {
        value
    } else {
        value.saturating_add(step - remainder)
    }
}

/// Storage cost (in MIST) of the synthetic gas coin that the simulator wrote during the
/// estimation pass. The new `storage_rebate` on the written object equals the storage cost
/// (see `track_storage_mutation` / `collect_storage_and_rebate`) so we can read it directly
/// from the simulation's object set instead of re-deriving it from protocol parameters.
/// Returns 0 when the simulation didn't use a mock gas coin.
fn mock_gas_storage_cost(
    simulation_result: &sui_types::transaction_executor::SimulateTransactionResult,
) -> u64 {
    let Some(mock_gas_id) = simulation_result.mock_gas_id else {
        return 0;
    };
    // Both the input version (rebate 0, since the mock coin is fresh) and the written version
    // (rebate set to the storage cost) end up in `objects`. The written version always carries
    // the larger value, so taking the max is correct and avoids depending on iteration order.
    simulation_result
        .objects
        .iter()
        .filter(|o| o.id() == mock_gas_id)
        .map(|o| o.storage_rebate)
        .max()
        .unwrap_or(0)
}

fn select_gas(
    service: &RpcService,
    transaction: &mut sui_types::transaction::TransactionData,
    incremental_loading_rgp: Option<u64>,
    protocol_config: &ProtocolConfig,
) -> Result<()> {
    use sui_types::accumulator_root::AccumulatorValue;
    use sui_types::balance::Balance;
    use sui_types::base_types::SequenceNumber;
    use sui_types::coin_reservation::CoinReservationResolver;
    use sui_types::coin_reservation::ParsedDigest;
    use sui_types::coin_reservation::ParsedObjectRefWithdrawal;
    use sui_types::gas_coin::GAS;
    use sui_types::gas_coin::GasCoin;
    use sui_types::transaction::Command;
    use sui_types::transaction::TransactionDataAPI;
    use sui_types::transaction::TransactionExpiration;

    let reader = &service.reader;

    let owner = transaction.gas_data().owner;
    let budget = transaction.gas_data().budget;

    let gas_coin_used = transaction
        .kind()
        .iter_commands()
        .any(Command::is_gas_coin_used);
    let address_balance = reader
        .lookup_address_balance(owner, GAS::type_())
        .map(|balance| {
            // Sum up the explicit SUI reservations (excluding the implicit gas payment) for the
            // `owner` so that we can deduct that from the available address balance. We use the
            // estimation variant to avoid double-counting: the gas budget is what we're trying to
            // satisfy, not a pre-existing reservation.
            let coin_resolver = CoinReservationResolver::new(reader.inner().clone());

            let reserved_sui = transaction
                .process_funds_withdrawals_for_estimation(service.chain_id, &coin_resolver)
                .ok()
                .and_then(|withdrawals| {
                    let sui_type = Balance::type_tag(GAS::type_tag());
                    let sui_account_id = AccumulatorValue::get_field_id(owner, &sui_type).ok()?;
                    withdrawals.get(&sui_account_id).map(|(amount, _)| *amount)
                })
                .unwrap_or(0);

            balance.saturating_sub(reserved_sui)
        });

    // If the gas coin isn't used and there is sufficient address balance budget to satisfy the
    // required budget then we will use the `owner`s address balance to pay for gas. Otherwise we
    // fallback to doing coin selection
    let selected_gas_value = if !gas_coin_used
        && let Some(address_balance) = address_balance
        && address_balance >= budget
    {
        // We probably don't need to do this, but explicitly clear out the payment to force using
        // Address balance
        transaction.gas_data_mut().payment.clear();

        let current_epoch = service.reader.inner().get_latest_checkpoint()?.epoch();

        if matches!(transaction.expiration(), TransactionExpiration::None) {
            *transaction.expiration_mut() = TransactionExpiration::ValidDuring {
                min_epoch: Some(current_epoch),
                max_epoch: Some(current_epoch.saturating_add(1)),
                min_timestamp: None,
                max_timestamp: None,
                chain: service.chain_id,
                nonce: rand::random(),
            };
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

            // Set expiration for address balance usage if not already set
            if matches!(transaction.expiration(), TransactionExpiration::None) {
                *transaction.expiration_mut() = TransactionExpiration::ValidDuring {
                    min_epoch: Some(current_epoch),
                    max_epoch: Some(current_epoch.saturating_add(1)),
                    min_timestamp: None,
                    max_timestamp: None,
                    chain: service.chain_id,
                    nonce: rand::random(),
                };
            }
        }

        transaction.gas_data_mut().payment = selected_gas;

        selected_gas_value
    };

    // When the caller asked us to top up the budget for the loading cost of the just-picked
    // payment objects, do so before the balance check. The simulation already counted the
    // synthetic gas coin's load, so charge for the rest. Coin reservations don't load real
    // gas-coin objects and are excluded.
    let final_budget = if let Some(rgp) = incremental_loading_rgp {
        let real_coins = transaction
            .gas_data()
            .payment
            .iter()
            .filter(|obj_ref| !ParsedDigest::is_coin_reservation_digest(&obj_ref.2))
            .count() as u64;
        let extra_loading_mist =
            compute_gas_loading_cost_mist(real_coins.saturating_sub(1), rgp, protocol_config);
        let new_budget = budget
            .saturating_add(extra_loading_mist)
            .min(protocol_config.max_tx_gas());
        transaction.gas_data_mut().budget = new_budget;
        new_budget
    } else {
        budget
    };

    if selected_gas_value >= final_budget {
        Ok(())
    } else {
        Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!(
                "Unable to perform gas selection due to insufficient SUI \
                balance (in address balance or coins) for account {owner} \
                to satisfy required budget {final_budget}."
            ),
        ))
    }
}

/// Returns true if the simulate request is eligible for auto gas_price=0 handling.
///
/// Requires: gasless enabled by protocol, caller did not set price or gas payment objects, tx is a
/// PTB that passes the structural gasless checks, and all loaded Move object inputs pass the
/// runtime gasless input check (`Coin<T>` with `T` allowlisted, AddressOwner/ConsensusAddressOwner).
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
    if request.transaction().gas_payment().price.is_some() {
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
