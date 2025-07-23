// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::reader::StateReader;
use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use itertools::Itertools;
use sui_protocol_config::ProtocolConfig;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2beta2::Bcs;
use sui_rpc::proto::sui::rpc::v2beta2::CommandOutput;
use sui_rpc::proto::sui::rpc::v2beta2::CommandResult;
use sui_rpc::proto::sui::rpc::v2beta2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2beta2::SimulateTransactionRequest;
use sui_rpc::proto::sui::rpc::v2beta2::SimulateTransactionResponse;
use sui_rpc::proto::sui::rpc::v2beta2::Transaction;
use sui_rpc::proto::sui::rpc::v2beta2::TransactionEffects;
use sui_rpc::proto::sui::rpc::v2beta2::TransactionEvents;
use sui_types::balance_change::derive_balance_changes;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::transaction::TransactionDataAPI;
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
                .into())
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
    // ever be committed to the chain)
    if request.do_gas_selection() && checks.enabled() {
        let budget = {
            let simulation_result = executor
                .simulate_transaction(transaction.clone(), TransactionChecks::Enabled)
                .map_err(anyhow::Error::from)?;

            let estimate = estimate_gas_budget_from_gas_cost(
                simulation_result.effects.gas_cost_summary(),
                reference_gas_price,
            );
            transaction.gas_data_mut().budget = estimate;
            estimate
        };

        // If the user didn't provide any gas payment we need to do gas selection now
        if transaction.gas_data().payment.is_empty() {
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
            let gas_coins = select_gas(
                &service.reader,
                transaction.gas_data().owner,
                budget,
                protocol_config.max_gas_payment_objects(),
                &input_objects,
            )?;
            transaction.gas_data_mut().payment = gas_coins;
        }
    }

    let SimulateTransactionResult {
        input_objects,
        output_objects,
        events,
        effects,
        execution_result,
        mock_gas_id: _,
    } = executor
        .simulate_transaction(transaction.clone(), checks)
        .map_err(anyhow::Error::from)?;

    let transaction = if let Some(submask) = read_mask.subtree("transaction") {
        let mut message = ExecutedTransaction::default();
        let transaction = sui_sdk_types::Transaction::try_from(transaction)?;

        let input_objects = input_objects.into_values().collect::<Vec<_>>();
        let output_objects = output_objects.into_values().collect::<Vec<_>>();

        message.balance_changes = read_mask
            .contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name)
            .then(|| {
                derive_balance_changes(&effects, &input_objects, &output_objects)
                    .into_iter()
                    .map(Into::into)
                    .collect()
            })
            .unwrap_or_default();

        message.effects = {
            let effects = sui_sdk_types::TransactionEffects::try_from(effects)?;
            submask
                .subtree(ExecutedTransaction::EFFECTS_FIELD.name)
                .map(|mask| TransactionEffects::merge_from(&effects, &mask))
        };

        message.events = submask
            .subtree(ExecutedTransaction::EVENTS_FIELD.name)
            .and_then(|mask| {
                events.map(|events| {
                    sui_sdk_types::TransactionEvents::try_from(events)
                        .map(|events| TransactionEvents::merge_from(events, &mask))
                })
            })
            .transpose()?;

        message.transaction = submask
            .subtree(ExecutedTransaction::TRANSACTION_FIELD.name)
            .map(|mask| Transaction::merge_from(transaction, &mask));

        Some(message)
    } else {
        None
    };

    let outputs = if read_mask.contains("outputs") {
        execution_result
            .into_iter()
            .flatten()
            .map(|(reference_outputs, return_values)| CommandResult {
                return_values: return_values
                    .into_iter()
                    .map(|(bcs, ty)| to_command_output(service, None, bcs, ty))
                    .collect(),
                mutated_by_ref: reference_outputs
                    .into_iter()
                    .map(|(arg, bcs, ty)| to_command_output(service, Some(arg), bcs, ty))
                    .collect(),
            })
            .collect()
    } else {
        Vec::new()
    };

    let response = SimulateTransactionResponse {
        transaction,
        outputs,
    };
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
            sui_types::proto_value::ProtoVisitorBuilder::new(
                service.config.max_json_move_value_size(),
            )
            .deserialize_value(&bcs, &layout)
            .map_err(|e| tracing::debug!("unable to convert to JSON: {e}"))
            .ok()
            .map(Box::new)
        });

    CommandOutput {
        argument: arg.map(Into::into),
        value: Some(Bcs {
            name: Some(ty.to_canonical_string(true)),
            value: Some(bcs.into()),
        }),
        json,
    }
}

/// Estimate the gas budget using the gas_cost_summary from a previous DryRun
///
/// The estimated gas budget is computed as following:
/// * the maximum between A and B, where:
///     A = computation cost + GAS_SAFE_OVERHEAD * reference gas price
///     B = computation cost + storage cost - storage rebate + GAS_SAFE_OVERHEAD * reference gas price
///     overhead
///
/// This gas estimate is computed similarly as in the TypeScript SDK
fn estimate_gas_budget_from_gas_cost(
    gas_cost_summary: &sui_types::gas::GasCostSummary,
    reference_gas_price: u64,
) -> u64 {
    const GAS_SAFE_OVERHEAD: u64 = 1000;

    let safe_overhead = GAS_SAFE_OVERHEAD * reference_gas_price;
    let computation_cost_with_overhead = gas_cost_summary.computation_cost + safe_overhead;

    let gas_usage = gas_cost_summary.net_gas_usage() + safe_overhead as i64;
    computation_cost_with_overhead.max(if gas_usage < 0 { 0 } else { gas_usage as u64 })
}

fn select_gas(
    reader: &StateReader,
    owner: SuiAddress,
    budget: u64,
    max_gas_payment_objects: u32,
    input_objects: &[ObjectID],
) -> Result<Vec<ObjectRef>> {
    use sui_types::gas_coin::GasCoin;

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

    if selected_gas_value >= budget {
        Ok(selected_gas)
    } else {
        Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!(
                "unable to select sufficient gas coins from account {owner} \
                    to satisfy required budget {budget}"
            ),
        ))
    }
}
