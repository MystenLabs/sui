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
use sui_types::transaction::TransactionDataAPI;
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

    // Perform budgest estimation and gas selection if requested and if TransactionChecks are enabled (it
    // makes no sense to do gas selection if checks are disabled because such a transaction can't
    // ever be committed to the chain).
    if request.do_gas_selection() && checks.enabled() {
        // At this point, the budget on the transaction can be set to one of the following:
        // - The budget from the request, if specified.
        // - The total balance of all of the gas payment coins (clamped to the protocol
        //   MAX_GAS_BUDGET) in the request if the budget was not
        //   specified but the gas payment coins were specified.
        // - Protocol MAX_GAS_BUDGET if the request did not specified neither gas payment or budget.
        //
        // If the request did not specify a budget, then simulate the transaction to get a budget estimate and
        // overwrite the resolved budget with the more accurate estimate.
        if request.transaction().gas_payment().budget.is_none()
            && request.transaction().bcs_opt().is_none()
        {
            let mut estimation_transaction = transaction.clone();
            estimation_transaction.gas_data_mut().payment = Vec::new();
            estimation_transaction.gas_data_mut().budget = protocol_config.max_tx_gas();

            let simulation_result = executor
                .simulate_transaction(
                    estimation_transaction,
                    TransactionChecks::Enabled,
                    true, /* allow mock gas coin */
                )
                .map_err(anyhow::Error::from)?;

            if !simulation_result.effects.status().is_ok() {
                return Err(RpcError::new(
                    tonic::Code::InvalidArgument,
                    format!(
                        "Budget estimation failed with status: {:?}.",
                        simulation_result.effects.status()
                    ),
                ));
            }

            let estimate = estimate_gas_budget_from_gas_cost(
                simulation_result.effects.gas_cost_summary(),
                reference_gas_price,
                request.transaction().gas_payment().objects.len(),
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
                protocol_config.max_gas_payment_objects(),
            )?;
        }
    }

    let allow_mock_gas_coin = checks.disabled() || !request.do_gas_selection();

    let SimulateTransactionResult {
        effects,
        events,
        objects,
        execution_result,
        mock_gas_id,
        unchanged_loaded_runtime_objects,
    } = executor
        .simulate_transaction(transaction.clone(), checks, allow_mock_gas_coin)
        .map_err(anyhow::Error::from)?;

    if !allow_mock_gas_coin && mock_gas_id.is_some() {
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
                    |object_id| {
                        objects
                            .iter()
                            .find(|o| o.id() == *object_id)
                            .map(|o| o.into())
                    },
                    &mask,
                )
            });

        message.events = submask
            .subtree(ExecutedTransaction::EVENTS_FIELD.name)
            .and_then(|mask| events.map(|events| service.render_events_to_proto(&events, &mask)));

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
                        .map(|o| service.render_object_to_proto(o, &mask))
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
    Ok(response)
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
            sui_types::proto_value::ProtoVisitor::new(service.config.max_json_move_value_size())
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
/// 1. Base cost from gas_cost_summary (computation + storage costs)
/// 2. Cost of loading gas payment objects (which weren't loaded during simulation)
/// 3. Rounding up to the protocol gas rounding step (typically 1000 MIST)
/// 4. Adding safe overhead buffer (1000 * reference_gas_price)
/// 5. Clamping to max_tx_gas protocol limit
fn estimate_gas_budget_from_gas_cost(
    gas_cost_summary: &sui_types::gas::GasCostSummary,
    reference_gas_price: u64,
    num_payment_objects_on_request: usize,
    protocol_config: &ProtocolConfig,
) -> u64 {
    const GAS_SAFE_OVERHEAD: u64 = 1000;

    // Calculate base estimate from gas cost summary (in MIST)
    let gas_usage = gas_cost_summary.net_gas_usage();
    let base_estimate_mist =
        gas_cost_summary
            .computation_cost
            .max(if gas_usage < 0 { 0 } else { gas_usage as u64 });

    // Calculate cost of loading gas payment objects.
    // Subtract 1 because the simulation already loaded one ephemeral gas coin.
    let num_payment_objects_for_estimation = {
        let total = if num_payment_objects_on_request == 0 {
            protocol_config.max_gas_payment_objects() as u64
        } else {
            num_payment_objects_on_request as u64
        };
        total.saturating_sub(1)
    };

    // Calculate gas loading cost in gas units
    let gas_loading_cost_units = num_payment_objects_for_estimation
        .saturating_mul(GAS_COIN_SIZE_BYTES)
        .saturating_mul(protocol_config.obj_access_cost_read_per_byte());

    // Round up to the nearest gas rounding step (in gas units)
    let rounded_gas_loading_cost_units =
        if let Some(step) = protocol_config.gas_rounding_step_as_option() {
            round_up_to_nearest(gas_loading_cost_units, step)
        } else {
            gas_loading_cost_units
        };

    // Convert gas loading cost to MIST
    let gas_loading_cost_mist = rounded_gas_loading_cost_units.saturating_mul(reference_gas_price);

    // Calculate safe overhead buffer in MIST
    let safe_overhead_mist = GAS_SAFE_OVERHEAD.saturating_mul(reference_gas_price);

    // Add all together: base (MIST) + loading (MIST) + overhead (MIST)
    let estimate_mist = base_estimate_mist
        .saturating_add(gas_loading_cost_mist)
        .saturating_add(safe_overhead_mist);

    // Clamp to max_tx_gas to ensure we don't exceed the protocol limit
    estimate_mist.min(protocol_config.max_tx_gas())
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

fn select_gas(
    service: &RpcService,
    transaction: &mut sui_types::transaction::TransactionData,
    max_gas_payment_objects: u32,
) -> Result<()> {
    use sui_types::gas_coin::GAS;
    use sui_types::gas_coin::GasCoin;
    use sui_types::transaction::Command;
    use sui_types::transaction::Reservation;
    use sui_types::transaction::TransactionDataAPI;
    use sui_types::transaction::TransactionExpiration;
    use sui_types::transaction::WithdrawalTypeArg;

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
            // Sum up the total SUI reservations for the `owner` so that we can deduct that from the
            // available address balance for determining if an account as sufficient funds.
            let reserved_sui = transaction
                .get_funds_withdrawals()
                .into_iter()
                .filter_map(|w| {
                    // Skip if this withdrawal isn't for the gas owner
                    if w.owner_for_withdrawal(&*transaction) != owner {
                        return None;
                    }

                    // Skip if this withdrawal isn't for SUI
                    let WithdrawalTypeArg::Balance(coin_type) = &w.type_arg;
                    if !GAS::is_gas_type(coin_type) {
                        return None;
                    }

                    match w.reservation {
                        Reservation::MaxAmountU64(value) => Some(value),
                    }
                })
                .sum::<u64>();

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

        *transaction.expiration_mut() = TransactionExpiration::ValidDuring {
            min_epoch: Some(current_epoch),
            max_epoch: Some(current_epoch.saturating_add(1)),
            min_timestamp: None,
            max_timestamp: None,
            chain: service.chain_id,
            nonce: rand::random(), // generate a random nonce to use
        };

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
            .take(max_gas_payment_objects as usize);

        let mut selected_gas = vec![];
        let mut selected_gas_value = 0;

        for maybe_coin in gas_coins {
            let (object_ref, value) =
                maybe_coin.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
            selected_gas.push(object_ref);
            selected_gas_value += value;
        }

        transaction.gas_data_mut().payment = selected_gas;

        selected_gas_value
    };

    if selected_gas_value >= budget {
        Ok(())
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
