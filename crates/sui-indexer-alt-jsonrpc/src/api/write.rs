// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::str::FromStr;

use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::types::ErrorObject;
use jsonrpsee::types::error::INTERNAL_ERROR_CODE;
use jsonrpsee::types::error::INVALID_PARAMS_CODE;
use move_core_types::annotated_value::MoveDatatypeLayout;
use move_core_types::annotated_value::MoveTypeLayout;
use prost_types::FieldMask;
use sui_indexer_alt_reader::fullnode_client::FullnodeClient;
use sui_json_rpc_types::BalanceChange as SuiBalanceChange;
use sui_json_rpc_types::DryRunTransactionBlockResponse;
use sui_json_rpc_types::ObjectChange as SuiObjectChange;
use sui_json_rpc_types::SuiEvent;
use sui_json_rpc_types::SuiTransactionBlock;
use sui_json_rpc_types::SuiTransactionBlockData;
use sui_json_rpc_types::SuiTransactionBlockEffects;
use sui_json_rpc_types::SuiTransactionBlockEvents;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::TypeTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::ToFromBytes;
use sui_types::digests::ObjectDigest;
use sui_types::effects::IDOperation;
use sui_types::effects::ObjectChange;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::event::Event;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction_driver_types::ExecuteTransactionRequestType;

use crate::api::rpc_module::RpcModule;
use crate::context::Context;
use crate::error::invalid_params;

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

    /// Return transaction execution effects including the gas cost summary,
    /// while the effects are not committed to the chain.
    #[method(name = "dryRunTransactionBlock")]
    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse>;
}

pub(crate) struct Write {
    client: FullnodeClient,
    context: Context,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("WaitForLocalExecution mode is deprecated")]
    DeprecatedWaitForLocalExecution,
}

impl Write {
    pub(crate) fn new(client: FullnodeClient, context: Context) -> Self {
        Self { client, context }
    }
}

#[async_trait::async_trait]
impl WriteApiServer for Write {
    async fn execute_transaction_block(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        options: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        if let Some(ExecuteTransactionRequestType::WaitForLocalExecution) = request_type {
            return Err(invalid_params(Error::DeprecatedWaitForLocalExecution).into());
        }

        let options = options.unwrap_or_default();
        let tx_data: TransactionData =
            bcs::from_bytes(&tx_bytes.to_vec().map_err(invalid_params_err)?).map_err(|e| {
                invalid_params_err(anyhow::anyhow!(
                    "Failed to deserialize TransactionData: {e}"
                ))
            })?;
        let tx_digest = tx_data.digest();

        let parsed_sigs = parse_signatures(&signatures)?;

        let read_mask = build_execute_read_mask(&options);

        let response = self
            .client
            .execute_transaction(tx_data.clone(), parsed_sigs.clone(), Some(read_mask))
            .await
            .map_err(grpc_error_to_error_object)?;

        let executed_tx = response
            .transaction
            .as_ref()
            .ok_or_else(|| internal_err("Missing transaction in gRPC response"))?;

        let mut result = SuiTransactionBlockResponse::new(tx_digest);
        result.checkpoint = executed_tx.checkpoint;
        result.timestamp_ms = executed_tx
            .timestamp
            .and_then(|ts| sui_rpc::proto::proto_to_timestamp_ms(ts).ok());

        if options.show_input {
            let sui_tx_data = SuiTransactionBlockData::try_from_with_package_resolver(
                tx_data.clone(),
                self.context.package_resolver(),
            )
            .await
            .map_err(|e| internal_err(format!("Failed to convert transaction data: {e}")))?;
            result.transaction = Some(SuiTransactionBlock {
                data: sui_tx_data,
                tx_signatures: parsed_sigs.clone(),
            });
        }

        if options.show_raw_input {
            result.raw_transaction = bcs::to_bytes(&tx_data)
                .map_err(|e| internal_err(format!("Failed to serialize transaction: {e}")))?;
        }

        if options.show_effects || options.show_raw_effects || options.show_object_changes {
            let effects_bcs = executed_tx
                .effects
                .as_ref()
                .and_then(|e| e.bcs.as_ref())
                .ok_or_else(|| internal_err("Missing effects.bcs in gRPC response"))?;

            if options.show_raw_effects {
                result.raw_effects = effects_bcs.value().to_vec();
            }

            let effects: TransactionEffects = effects_bcs
                .deserialize()
                .map_err(|e| internal_err(format!("Failed to deserialize effects: {e}")))?;

            if options.show_effects {
                let sui_effects: SuiTransactionBlockEffects = effects
                    .clone()
                    .try_into()
                    .map_err(|e| internal_err(format!("Failed to convert effects: {e}")))?;
                result.effects = Some(sui_effects);
            }

            if options.show_object_changes {
                result.object_changes = Some(
                    build_object_changes(&tx_data, &effects, executed_tx).map_err(|e| {
                        internal_err(format!("Failed to build object changes: {e}"))
                    })?,
                );
            }
        }

        if options.show_events {
            result.events = Some(
                build_events(&self.context, tx_digest, executed_tx)
                    .await
                    .map_err(|e| internal_err(format!("Failed to build events: {e}")))?,
            );
        }

        if options.show_balance_changes {
            result.balance_changes = Some(
                build_balance_changes(executed_tx)
                    .map_err(|e| internal_err(format!("Failed to build balance changes: {e}")))?,
            );
        }

        Ok(result)
    }

    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse> {
        let raw_tx_bytes = tx_bytes.to_vec().map_err(invalid_params_err)?;
        let tx_data: TransactionData = bcs::from_bytes(&raw_tx_bytes).map_err(|e| {
            invalid_params_err(anyhow::anyhow!(
                "Failed to deserialize TransactionData: {e}"
            ))
        })?;

        let mut proto_tx = proto::Transaction::default();
        proto_tx.bcs =
            Some(proto::Bcs::serialize(&tx_data).map_err(|e| {
                internal_err(format!("Failed to serialize transaction for gRPC: {e}"))
            })?);

        let read_mask = FieldMask::from_paths([
            "transaction.effects.bcs",
            "transaction.transaction.bcs",
            "transaction.events.bcs",
            "transaction.balance_changes",
            "transaction.effects.changed_objects",
            "transaction.objects.objects.bcs",
            "transaction.checkpoint",
            "transaction.timestamp",
            "suggested_gas_price",
        ]);

        let response = self
            .client
            .simulate_transaction(proto_tx, true, false, Some(read_mask))
            .await
            .map_err(grpc_error_to_error_object)?;

        let executed_tx = response
            .transaction
            .as_ref()
            .ok_or_else(|| internal_err("Missing transaction in dry run gRPC response"))?;

        let effects_bcs = executed_tx
            .effects
            .as_ref()
            .and_then(|e| e.bcs.as_ref())
            .ok_or_else(|| internal_err("Missing effects.bcs in dry run gRPC response"))?;

        let effects: TransactionEffects = effects_bcs
            .deserialize()
            .map_err(|e| internal_err(format!("Failed to deserialize effects: {e}")))?;

        let sui_effects: SuiTransactionBlockEffects = effects
            .clone()
            .try_into()
            .map_err(|e| internal_err(format!("Failed to convert effects: {e}")))?;

        let tx_digest = tx_data.digest();
        let events = build_events(&self.context, tx_digest, executed_tx)
            .await
            .map_err(|e| internal_err(format!("Failed to build events: {e}")))?;

        let object_changes = build_object_changes(&tx_data, &effects, executed_tx)
            .map_err(|e| internal_err(format!("Failed to build object changes: {e}")))?;

        let balance_changes = build_balance_changes(executed_tx)
            .map_err(|e| internal_err(format!("Failed to build balance changes: {e}")))?;

        let input = SuiTransactionBlockData::try_from_with_package_resolver(
            tx_data,
            self.context.package_resolver(),
        )
        .await
        .map_err(|e| internal_err(format!("Failed to convert transaction data: {e}")))?;

        Ok(DryRunTransactionBlockResponse {
            effects: sui_effects,
            events,
            object_changes,
            balance_changes,
            input,
            execution_error_source: None,
            suggested_gas_price: response.suggested_gas_price,
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

fn parse_signatures(signatures: &[Base64]) -> RpcResult<Vec<GenericSignature>> {
    signatures
        .iter()
        .enumerate()
        .map(|(i, sig)| {
            let bytes = sig.to_vec().map_err(|e| {
                invalid_params_err(anyhow::anyhow!("Invalid base64 in signature {i}: {e}"))
            })?;
            GenericSignature::from_bytes(&bytes)
                .map_err(|e| invalid_params_err(anyhow::anyhow!("Invalid signature {i}: {e}")))
        })
        .collect()
}

fn build_execute_read_mask(options: &SuiTransactionBlockResponseOptions) -> FieldMask {
    let mut paths = vec!["checkpoint", "timestamp"];

    if options.show_effects || options.show_raw_effects || options.show_object_changes {
        paths.push("effects.bcs");
    }

    if options.show_object_changes {
        paths.push("effects.changed_objects");
        paths.push("objects.objects.bcs");
    }

    if options.show_events {
        paths.push("events.bcs");
    }

    if options.show_balance_changes {
        paths.push("balance_changes");
    }

    FieldMask::from_paths(paths)
}

fn build_balance_changes(
    executed_tx: &proto::ExecutedTransaction,
) -> anyhow::Result<Vec<SuiBalanceChange>> {
    executed_tx
        .balance_changes
        .iter()
        .map(|bc| {
            let addr: SuiAddress = bc
                .address
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing address in balance change"))?
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid owner address: {e}"))?;
            let owner = Owner::AddressOwner(addr);
            let coin_type = TypeTag::from_str(
                bc.coin_type
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Missing coin_type in balance change"))?,
            )
            .map_err(|e| anyhow::anyhow!("Invalid coin type: {e}"))?;
            let amount: i128 = bc
                .amount
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing amount in balance change"))?
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid balance change amount: {e}"))?;

            Ok(SuiBalanceChange {
                owner,
                coin_type,
                amount,
            })
        })
        .collect()
}

async fn build_events(
    ctx: &Context,
    tx_digest: sui_types::digests::TransactionDigest,
    executed_tx: &proto::ExecutedTransaction,
) -> anyhow::Result<SuiTransactionBlockEvents> {
    let events_bcs = executed_tx.events.as_ref().and_then(|e| e.bcs.as_ref());

    let events: Vec<Event> = match events_bcs {
        Some(bcs) => bcs.deserialize()?,
        None => vec![],
    };

    let mut sui_events = Vec::with_capacity(events.len());
    for (ix, event) in events.into_iter().enumerate() {
        let layout = match ctx
            .package_resolver()
            .type_layout(event.type_.clone().into())
            .await?
        {
            MoveTypeLayout::Struct(s) => MoveDatatypeLayout::Struct(s),
            MoveTypeLayout::Enum(e) => MoveDatatypeLayout::Enum(e),
            _ => anyhow::bail!(
                "Event {ix} is not a struct or enum: {}",
                event.type_.to_canonical_string(true)
            ),
        };
        sui_events.push(SuiEvent::try_from(
            event, tx_digest, ix as u64, None, layout,
        )?);
    }

    Ok(SuiTransactionBlockEvents { data: sui_events })
}

fn build_object_changes(
    tx_data: &TransactionData,
    effects: &TransactionEffects,
    executed_tx: &proto::ExecutedTransaction,
) -> anyhow::Result<Vec<SuiObjectChange>> {
    let native_changes = effects.object_changes();

    // Build a map of (ObjectID, version) → Object from the proto objects
    let mut objects: HashMap<(ObjectID, u64), Object> = HashMap::new();
    if let Some(object_set) = &executed_tx.objects {
        for proto_obj in &object_set.objects {
            if let Some(bcs) = &proto_obj.bcs {
                let obj: Object = bcs.deserialize()?;
                objects.insert((obj.id(), obj.version().value()), obj);
            }
        }
    }

    let fetch_object = |id: ObjectID,
                        v: Option<SequenceNumber>,
                        d: Option<ObjectDigest>|
     -> anyhow::Result<Option<(Object, ObjectDigest)>> {
        let Some(v) = v else { return Ok(None) };
        let Some(d) = d else { return Ok(None) };
        let key = (id, v.value());
        match objects.get(&key) {
            Some(o) => Ok(Some((o.clone(), d))),
            None => Ok(None),
        }
    };

    let mut changes = Vec::with_capacity(native_changes.len());
    for change in &native_changes {
        let &ObjectChange {
            id: object_id,
            id_operation,
            input_version,
            input_digest,
            output_version,
            output_digest,
            ..
        } = change;

        let input = fetch_object(object_id, input_version, input_digest)?;
        let output = fetch_object(object_id, output_version, output_digest)?;

        use IDOperation as ID;
        let sui_change = match (id_operation, &input, &output) {
            (ID::Created, Some((i, _)), _) => anyhow::bail!(
                "Unexpected input version {} for object {object_id} created by transaction",
                i.version().value(),
            ),

            (ID::Deleted, _, Some((o, _))) => anyhow::bail!(
                "Unexpected output version {} for object {object_id} deleted by transaction",
                o.version().value(),
            ),

            (ID::Created, _, None) => continue,
            (ID::None, None, _) => continue,
            (ID::None, _, Some((o, _))) if o.is_package() => continue,
            (ID::Deleted, None, _) => continue,

            (ID::Created, _, Some((o, d))) if o.is_package() => SuiObjectChange::Published {
                package_id: object_id,
                version: o.version(),
                digest: *d,
                modules: o
                    .data
                    .try_as_package()
                    .unwrap()
                    .serialized_module_map()
                    .keys()
                    .cloned()
                    .collect(),
            },

            (ID::Created, _, Some((o, d))) => SuiObjectChange::Created {
                sender: tx_data.sender(),
                owner: o.owner().clone(),
                object_type: o
                    .struct_tag()
                    .ok_or_else(|| anyhow::anyhow!("No type for object {object_id}"))?,
                object_id,
                version: o.version(),
                digest: *d,
            },

            (ID::None, Some((i, _)), Some((o, od))) if i.owner() != o.owner() => {
                SuiObjectChange::Transferred {
                    sender: tx_data.sender(),
                    recipient: o.owner().clone(),
                    object_type: o
                        .struct_tag()
                        .ok_or_else(|| anyhow::anyhow!("No type for object {object_id}"))?,
                    object_id,
                    version: o.version(),
                    digest: *od,
                }
            }

            (ID::None, Some((i, _)), Some((o, od))) => SuiObjectChange::Mutated {
                sender: tx_data.sender(),
                owner: o.owner().clone(),
                object_type: o
                    .struct_tag()
                    .ok_or_else(|| anyhow::anyhow!("No type for object {object_id}"))?,
                object_id,
                version: o.version(),
                previous_version: i.version(),
                digest: *od,
            },

            (ID::None, Some((i, _)), None) => SuiObjectChange::Wrapped {
                sender: tx_data.sender(),
                object_type: i
                    .struct_tag()
                    .ok_or_else(|| anyhow::anyhow!("No type for object {object_id}"))?,
                object_id,
                version: effects.lamport_version(),
            },

            (ID::Deleted, Some((i, _)), None) => SuiObjectChange::Deleted {
                sender: tx_data.sender(),
                object_type: i
                    .struct_tag()
                    .ok_or_else(|| anyhow::anyhow!("No type for object {object_id}"))?,
                object_id,
                version: effects.lamport_version(),
            },
        };
        changes.push(sui_change);
    }

    Ok(changes)
}

fn grpc_error_to_error_object(
    error: sui_indexer_alt_reader::fullnode_client::Error,
) -> ErrorObject<'static> {
    use sui_indexer_alt_reader::fullnode_client::Error;
    match error {
        Error::GrpcExecutionError(status)
            if matches!(
                status.code(),
                tonic::Code::InvalidArgument | tonic::Code::NotFound
            ) =>
        {
            ErrorObject::owned(
                INVALID_PARAMS_CODE,
                status.message().to_string(),
                None::<()>,
            )
        }
        Error::NotConfigured => {
            ErrorObject::owned(INTERNAL_ERROR_CODE, error.to_string(), None::<()>)
        }
        _ => ErrorObject::owned(INTERNAL_ERROR_CODE, error.to_string(), None::<()>),
    }
}

fn invalid_params_err(err: impl std::fmt::Display) -> ErrorObject<'static> {
    ErrorObject::owned(INVALID_PARAMS_CODE, "Invalid params", Some(err.to_string()))
}

fn internal_err(msg: impl Into<String>) -> ErrorObject<'static> {
    ErrorObject::owned(INTERNAL_ERROR_CODE, msg.into(), None::<()>)
}
