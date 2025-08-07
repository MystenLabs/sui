// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::Base64;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Neg;

use sui_indexer_alt_jsonrpc::{api::rpc_module::RpcModule, error::invalid_params};
use sui_json_rpc_types::{
    BalanceChange, DevInspectArgs, DevInspectResults, DryRunTransactionBlockResponse, ObjectChange,
    SuiTransactionBlockData, SuiTransactionBlockEffects, SuiTransactionBlockEvents,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::balance_change::derive_balance_changes;
use sui_types::digests::ObjectDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::ExecutionStatus;
use sui_types::gas_coin::GAS;
use sui_types::object::Object;
use sui_types::sui_serde::BigInt;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    effects::{ObjectRemoveKind, TransactionEffects},
    object::Owner,
    storage::WriteKind,
    transaction::{InputObjectKind, TransactionData, TransactionDataAPI},
    transaction_driver_types::ExecuteTransactionRequestType,
};

use crate::execution;
use crate::rpc::object_provider::{ObjectProvider, ObjectProviderCache};

#[open_rpc(namespace = "sui", tag = "Write API")]
#[rpc(server, client, namespace = "sui")]
pub trait WriteApi {
    /// Execute the transaction with options to show different information in the response.
    /// The only supported request type is `WaitForEffectsCert`: waits for TransactionEffectsCert and then return to client.
    /// `WaitForLocalExecution` mode has been deprecated.
    #[method(name = "executeTransactionBlock")]
    async fn execute_transaction_block(
        &self,
        /// BCS serialized transaction data bytes without its type tag, as base-64 encoded string.
        tx_bytes: Base64,
        /// A list of signatures (`flag || signature || pubkey` bytes, as base-64 encoded string). Signature is committed to the intent message of the transaction data, as base-64 encoded string.
        signatures: Vec<Base64>,
        /// options for specifying the content to be returned
        options: Option<SuiTransactionBlockResponseOptions>,
        /// The request type, derived from `SuiTransactionBlockResponseOptions` if None
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> RpcResult<SuiTransactionBlockResponse>;

    /// Runs the transaction in dev-inspect mode. Which allows for nearly any
    /// transaction (or Move call) with any arguments. Detailed results are
    /// provided, including both the transaction effects and any return values.
    #[method(name = "devInspectTransactionBlock")]
    async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        /// BCS encoded TransactionKind(as opposed to TransactionData, which include gasBudget and gasPrice)
        tx_bytes: Base64,
        /// Gas is not charged, but gas usage is still calculated. Default to use reference gas price
        gas_price: Option<BigInt<u64>>,
        /// The epoch to perform the call. Will be set from the system state object if not provided
        epoch: Option<BigInt<u64>>,
        /// Additional arguments including gas_budget, gas_objects, gas_sponsor and skip_checks.
        additional_args: Option<DevInspectArgs>,
    ) -> RpcResult<DevInspectResults>;

    /// Return transaction execution effects including the gas cost summary,
    /// while the effects are not committed to the chain.
    #[method(name = "dryRunTransactionBlock")]
    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse>;
}

pub(crate) struct Write(pub crate::context::Context);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("WaitForLocalExecution mode is deprecated")]
    DeprecatedWaitForLocalExecution,
    #[error("Invalid base64: {0}")]
    InvalidBase64(String),
    #[error("Failed to decode transaction data: {0}")]
    DecodeErr(String),
    #[error("Failed to execute transaction: {0}")]
    ExecutionErr(String),
    #[error("Failed to convert: {0}")]
    ConversionErr(String),
    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

#[async_trait::async_trait]
impl WriteApiServer for Write {
    async fn execute_transaction_block(
        &self,
        tx_bytes: Base64,
        _signatures: Vec<Base64>,
        options: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        if let Some(ExecuteTransactionRequestType::WaitForLocalExecution) = request_type {
            return Err(invalid_params(Error::DeprecatedWaitForLocalExecution).into());
        }

        // Parse transaction bytes
        let tx_data = parse_tx_bytes(&tx_bytes)?;

        // Execute using shared executor
        let execution::ExecutionResult { effects, .. } =
            execution::execute_transaction(&self.0, tx_data)
                .await
                .map_err(|e| invalid_params(Error::ExecutionErr(format!("{:?}", e))))?;

        // Build the response based on options
        let options = options.unwrap_or_default();
        let mut response = SuiTransactionBlockResponse::new(*effects.transaction_digest());

        if options.show_effects {
            response.effects = Some(
                SuiTransactionBlockEffects::try_from(effects.clone())
                    .map_err(|e| invalid_params(Error::ConversionErr(format!("effects: {}", e))))?,
            );
        }

        if options.show_raw_input {
            response.raw_transaction = tx_bytes.to_vec().unwrap_or_default();
        }

        Ok(response)
    }

    async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<BigInt<u64>>,
        _epoch: Option<BigInt<u64>>,
        additional_args: Option<DevInspectArgs>,
    ) -> RpcResult<DevInspectResults> {
        // Parse transaction bytes
        let tx_data = parse_tx_bytes(&tx_bytes)?;
        let transaction_kind = tx_data.kind().clone();

        // Fetch input objects using shared executor
        {
            let mut simulacrum = self.0.simulacrum.write().await;
            let data_store = simulacrum.store_mut();
            execution::fetch_input_objects(&self.0, data_store, &tx_data)
                .await
                .map_err(|e| invalid_params(Error::ExecutionErr(format!("{:?}", e))))?;
        }

        let DevInspectArgs {
            gas_budget,
            gas_sponsor,
            gas_objects,
            show_raw_txn_data_and_effects,
            skip_checks,
        } = additional_args.unwrap_or_default();

        let simulacrum = self.0.simulacrum.write().await;
        let (inner_temp_store, effects, events, raw_txn_data, raw_events, error) = simulacrum
            .dev_inspect(
                sender_address,
                transaction_kind,
                gas_price.map(|x| *x),
                gas_budget.map(|x| *x),
                gas_sponsor,
                gas_objects,
                show_raw_txn_data_and_effects,
                skip_checks,
            )
            .unwrap();

        let mut resolver = simulacrum.create_layout_resolver(&inner_temp_store);

        let dev_inspect_results = DevInspectResults::new(
            effects,
            events,
            error,
            raw_txn_data,
            raw_events,
            resolver.as_mut(),
        )
        .unwrap();

        Ok(dev_inspect_results)
    }

    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse> {
        // Parse transaction bytes
        let tx_data = parse_tx_bytes(&tx_bytes)?;
        let input_objs = tx_data
            .input_objects()
            .map_err(|e| invalid_params(Error::ExecutionErr(e.to_string())))?;
        let sender = tx_data.sender();

        // Dry run using shared executor
        let execution::DryRunResult {
            inner_temp_store,
            effects,
            mock_gas,
            execution_result,
        } = execution::dry_run_transaction(&self.0, tx_data.clone())
            .await
            .map_err(|e| invalid_params(Error::ExecutionErr(format!("{:?}", e))))?;

        let modified_at_versions = effects.modified_at_versions();
        let all_changed_objects = effects.all_changed_objects();
        let all_removed_objects = effects.all_removed_objects();

        let simulacrum = self.0.simulacrum.read().await;
        let data_store = simulacrum.store_static();

        let written_with_kind = effects
            .created()
            .into_iter()
            .map(|(oref, _)| (oref, WriteKind::Create))
            .chain(
                effects
                    .unwrapped()
                    .into_iter()
                    .map(|(oref, _)| (oref, WriteKind::Unwrap)),
            )
            .chain(
                effects
                    .mutated()
                    .into_iter()
                    .map(|(oref, _)| (oref, WriteKind::Mutate)),
            )
            .map(|(oref, kind)| {
                let obj = inner_temp_store.written.get(&oref.0).unwrap();
                (oref.0, (oref, obj.clone(), kind))
            })
            .collect();

        let object_cache =
            ObjectProviderCache::new_with_cache(data_store.clone(), written_with_kind);

        let object_changes = get_object_changes(
            &object_cache,
            &effects,
            sender,
            modified_at_versions,
            all_changed_objects,
            all_removed_objects,
        )
        .await
        .map_err(|e| invalid_params(Error::ExecutionErr(e.to_string())))?;

        let balance_changes =
            get_balance_changes_from_effect(&object_cache, &effects, input_objs, mock_gas)
                .await
                .map_err(|e| invalid_params(Error::ExecutionErr(e.to_string())))?;

        let execution_error_source = execution_result
            .as_ref()
            .err()
            .and_then(|e| e.source().as_ref().map(|e| e.to_string()));

        let module_cache = sui_types::inner_temporary_store::TemporaryModuleResolver::new(
            &inner_temp_store,
            data_store.clone(),
        );

        let input =
            SuiTransactionBlockData::try_from_with_module_cache(tx_data, &module_cache).unwrap();

        Ok(DryRunTransactionBlockResponse {
            effects: effects.clone().try_into().unwrap(),
            events: SuiTransactionBlockEvents { data: vec![] },
            object_changes,
            balance_changes,
            input,
            execution_error_source,
            suggested_gas_price: None,
        })
    }
}

impl RpcModule for Write {
    fn schema(&self) -> Module {
        WriteApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

/// Parse base64-encoded transaction bytes into TransactionData.
fn parse_tx_bytes(tx_bytes: &Base64) -> RpcResult<TransactionData> {
    let tx_data_decoded = tx_bytes
        .to_vec()
        .map_err(|e| invalid_params(Error::InvalidBase64(e.to_string())))?;
    Ok(bcs::from_bytes::<TransactionData>(&tx_data_decoded)
        .map_err(|e| invalid_params(Error::DecodeErr(e.to_string())))?)
}

pub async fn get_object_changes<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    _effects: &TransactionEffects,
    sender: SuiAddress,
    modified_at_versions: Vec<(ObjectID, SequenceNumber)>,
    all_changed_objects: Vec<(ObjectRef, Owner, WriteKind)>,
    all_removed_objects: Vec<(ObjectRef, ObjectRemoveKind)>,
) -> Result<Vec<ObjectChange>, anyhow::Error> {
    let mut object_changes = vec![];

    let modify_at_version = modified_at_versions.into_iter().collect::<BTreeMap<_, _>>();

    for ((object_id, version, digest), owner, kind) in all_changed_objects {
        println!(
            "Processing changed object: {:?}, kind: {:?}, version :{:?}",
            object_id, kind, version
        );
        let o = object_provider
            .get_object(&object_id, &version)
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Failed to get object {:?} at version {:?}",
                    object_id,
                    version,
                )
            })?;
        if let Some(type_) = o.type_() {
            let object_type = type_.clone().into();

            match kind {
                WriteKind::Mutate => object_changes.push(ObjectChange::Mutated {
                    sender,
                    owner,
                    object_type,
                    object_id,
                    version,
                    // modify_at_version should always be available for mutated object
                    previous_version: modify_at_version
                        .get(&object_id)
                        .cloned()
                        .unwrap_or_default(),
                    digest,
                }),
                WriteKind::Create => object_changes.push(ObjectChange::Created {
                    sender,
                    owner,
                    object_type,
                    object_id,
                    version,
                    digest,
                }),
                _ => {}
            }
        } else if let Some(p) = o.data.try_as_package()
            && kind == WriteKind::Create
        {
            object_changes.push(ObjectChange::Published {
                package_id: p.id(),
                version: p.version(),
                digest,
                modules: p.serialized_module_map().keys().cloned().collect(),
            })
        };
    }

    for ((id, version, _), kind) in all_removed_objects {
        let o = object_provider
            .find_object_lt_or_eq_version(&id, &version)
            .await
            .map_err(|_| {
                anyhow::anyhow!("Failed to get object {:?} at version {:?}", id, version,)
            })?;
        if let Some(o) = o
            && let Some(type_) = o.type_()
        {
            let object_type = type_.clone().into();
            match kind {
                ObjectRemoveKind::Delete => object_changes.push(ObjectChange::Deleted {
                    sender,
                    object_type,
                    object_id: id,
                    version,
                }),
                ObjectRemoveKind::Wrap => object_changes.push(ObjectChange::Wrapped {
                    sender,
                    object_type,
                    object_id: id,
                    version,
                }),
            }
        };
    }

    Ok(object_changes)
}

pub async fn get_balance_changes_from_effect<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    effects: &TransactionEffects,
    input_objs: Vec<InputObjectKind>,
    mocked_coin: Option<ObjectID>,
) -> RpcResult<Vec<BalanceChange>> {
    let (_, gas_owner) = effects.gas_object();

    // Only charge gas when tx fails, skip all object parsing
    if effects.status() != &ExecutionStatus::Success {
        return Ok(vec![BalanceChange {
            owner: gas_owner,
            coin_type: GAS::type_tag(),
            amount: effects.gas_cost_summary().net_gas_usage().neg() as i128,
        }]);
    }

    let all_mutated = effects
        .all_changed_objects()
        .into_iter()
        .filter_map(|((id, version, digest), _, _)| {
            if matches!(mocked_coin, Some(coin) if id == coin) {
                return None;
            }
            Some((id, version, Some(digest)))
        })
        .collect::<Vec<_>>();

    let input_objs_to_digest = input_objs
        .iter()
        .filter_map(|k| match k {
            InputObjectKind::ImmOrOwnedMoveObject(o) => Some((o.0, o.2)),
            InputObjectKind::MovePackage(_) | InputObjectKind::SharedMoveObject { .. } => None,
        })
        .collect::<HashMap<ObjectID, ObjectDigest>>();
    let unwrapped_then_deleted = effects
        .unwrapped_then_deleted()
        .iter()
        .map(|e| e.0)
        .collect::<HashSet<_>>();

    let modified_at_version = effects
        .modified_at_versions()
        .into_iter()
        .filter_map(|(id, version)| {
            if matches!(mocked_coin, Some(coin) if id == coin) {
                return None;
            }
            // We won't be able to get dynamic object from object provider today
            if unwrapped_then_deleted.contains(&id) {
                return None;
            }
            Some((id, version, input_objs_to_digest.get(&id).cloned()))
        })
        .collect::<Vec<_>>();
    // TODO forking: handle unwrap
    let input_coins = fetch_coins(object_provider, &modified_at_version)
        .await
        .map_err(|_| {
            Error::NotImplemented(
                "It looks like fetch coins is not implemented correctly in forking / rpc"
                    .to_string(),
            )
        })
        .unwrap();
    // TODO forking: handle unwrap
    let mutated_coins = fetch_coins(object_provider, &all_mutated)
        .await
        .map_err(|_| {
            Error::NotImplemented(
                "It looks like fetch coins is not implemented correctly in forking / rpc"
                    .to_string(),
            )
        })
        .unwrap();
    Ok(
        derive_balance_changes(effects, &input_coins, &mutated_coins)
            .into_iter()
            .map(|change| BalanceChange {
                owner: sui_types::object::Owner::AddressOwner(change.address),
                coin_type: change.coin_type,
                amount: change.amount,
            })
            .collect(),
    )
}

async fn fetch_coins<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    objects: &[(ObjectID, SequenceNumber, Option<ObjectDigest>)],
) -> Result<Vec<Object>, E> {
    let mut coins = vec![];
    for (id, version, digest_opt) in objects {
        let o = object_provider.get_object(id, version).await?;

        if let Some(type_) = o.type_()
            && type_.is_coin()
        {
            if let Some(digest) = digest_opt {
                // TODO: can we return Err here instead?
                assert_eq!(
                    *digest,
                    o.digest(),
                    "Object digest mismatch--got bad data from object_provider?"
                )
            }
            coins.push(o);
        }
    }
    Ok(coins)
}
