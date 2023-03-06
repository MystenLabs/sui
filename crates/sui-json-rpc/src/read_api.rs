// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use move_binary_format::normalized::{Module as NormalizedModule, Type};
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use move_core_types::value::{MoveStruct, MoveStructLayout, MoveValue};
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_types::collection_types::VecMap;
use sui_types::display::{DisplayCreatedEvent, DisplayObject};
use sui_types::error::UserInputError;
use sui_types::intent::{AppId, Intent, IntentMessage, IntentScope, IntentVersion};
use tap::TapFallible;

use crate::api::ReadApiServer;
use fastcrypto::encoding::Base64;
use jsonrpsee::RpcModule;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{
    Checkpoint, CheckpointId, DynamicFieldPage, MoveFunctionArgType, ObjectValueKind, Page,
    SuiEvent, SuiMoveNormalizedFunction, SuiMoveNormalizedModule, SuiMoveNormalizedStruct,
    SuiMoveStruct, SuiMoveValue, SuiObjectDataOptions, SuiObjectInfo, SuiObjectResponse,
    SuiPastObjectResponse, SuiTransactionEvents, SuiTransactionResponse, TransactionsPage,
};
use sui_open_rpc::Module;
use sui_types::base_types::{
    ObjectID, SequenceNumber, SuiAddress, TransactionDigest, TxSequenceNumber,
};
use sui_types::crypto::sha3_hash;
use sui_types::messages::{TransactionData, TransactionEffectsAPI};
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
    CheckpointSummary,
};
use sui_types::move_package::normalize_modules;
use sui_types::object::{Data, Object, ObjectRead, PastObjectRead};
use sui_types::query::{EventQuery, TransactionQuery};

use sui_types::dynamic_field::DynamicFieldName;
use tracing::debug;

use crate::api::cap_page_limit;
use crate::error::Error;
use crate::SuiRpcModule;

use crate::api::QUERY_MAX_RESULT_LIMIT;

const MAX_DISPLAY_NESTED_LEVEL: usize = 10;

// An implementation of the read portion of the JSON-RPC interface intended for use in
// Fullnodes.
pub struct ReadApi {
    pub state: Arc<AuthorityState>,
}

impl ReadApi {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }

    fn get_checkpoint_internal(&self, id: CheckpointId) -> Result<Checkpoint, Error> {
        Ok(match id {
            CheckpointId::SequenceNumber(seq) => {
                let summary = self.state.get_checkpoint_summary_by_sequence_number(seq)?;
                let content = self.state.get_checkpoint_contents(summary.content_digest)?;
                (summary, content).into()
            }
            CheckpointId::Digest(digest) => {
                let summary = self.state.get_checkpoint_summary_by_digest(digest)?;
                let content = self.state.get_checkpoint_contents(summary.content_digest)?;
                (summary, content).into()
            }
        })
    }
}

#[async_trait]
impl ReadApiServer for ReadApi {
    async fn get_objects_owned_by_address(
        &self,
        address: SuiAddress,
    ) -> RpcResult<Vec<SuiObjectInfo>> {
        Ok(self
            .state
            .get_owner_objects(address)
            .map_err(|e| anyhow!("{e}"))?
            .into_iter()
            .map(SuiObjectInfo::from)
            .collect())
    }

    async fn get_dynamic_fields(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<DynamicFieldPage> {
        let limit = cap_page_limit(limit);
        let mut data = self
            .state
            .get_dynamic_fields(parent_object_id, cursor, limit + 1)
            .map_err(|e| anyhow!("{e}"))?;
        let next_cursor = data.get(limit).map(|info| info.object_id);
        data.truncate(limit);
        Ok(DynamicFieldPage { data, next_cursor })
    }

    async fn get_object_with_options(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        let object_read = self.state.get_object_read(&object_id).await.map_err(|e| {
            debug!(?object_id, "Failed to get object: {:?}", e);
            anyhow!("{e}")
        })?;
        let options = options.unwrap_or_default();

        match object_read {
            ObjectRead::NotExists(id) => Ok(SuiObjectResponse::NotExists(id)),
            ObjectRead::Exists(object_ref, o, layout) => {
                let display_fields = if options.show_display {
                    get_display_fields(self, &o, &layout).await?
                } else {
                    None
                };
                Ok(SuiObjectResponse::Exists(
                    (object_ref, o, layout, options, display_fields).try_into()?,
                ))
            }
            ObjectRead::Deleted(oref) => Ok(SuiObjectResponse::Deleted(oref.into())),
        }
    }

    async fn try_get_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse> {
        let past_read = self
            .state
            .get_past_object_read(&object_id, version)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        let options = options.unwrap_or_default();
        match past_read {
            PastObjectRead::ObjectNotExists(id) => Ok(SuiPastObjectResponse::ObjectNotExists(id)),
            PastObjectRead::VersionFound(object_ref, o, layout) => {
                let display_fields = if options.show_display {
                    get_display_fields(self, &o, &layout).await?
                } else {
                    None
                };
                Ok(SuiPastObjectResponse::VersionFound(
                    (object_ref, o, layout, options, display_fields).try_into()?,
                ))
            }
            PastObjectRead::ObjectDeleted(oref) => {
                Ok(SuiPastObjectResponse::ObjectDeleted(oref.into()))
            }
            PastObjectRead::VersionNotFound(id, seq_num) => {
                Ok(SuiPastObjectResponse::VersionNotFound(id, seq_num))
            }
            PastObjectRead::VersionTooHigh {
                object_id,
                asked_version,
                latest_version,
            } => Ok(SuiPastObjectResponse::VersionTooHigh {
                object_id,
                asked_version,
                latest_version,
            }),
        }
    }

    async fn get_dynamic_field_object(
        &self,
        parent_object_id: ObjectID,
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse> {
        let id = self
            .state
            .get_dynamic_field_object_id(parent_object_id, &name)
            .map_err(|e| anyhow!("{e}"))?
            .ok_or_else(|| {
                anyhow!("Cannot find dynamic field [{name:?}] for object [{parent_object_id}].")
            })?;
        // TODO(chris): add options to `get_dynamic_field_object` API as well
        self.get_object_with_options(id, Some(SuiObjectDataOptions::full_content()))
            .await
    }

    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        Ok(self.state.get_total_transaction_number()?)
    }

    async fn get_transactions_in_range(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> RpcResult<Vec<TransactionDigest>> {
        Ok(self
            .state
            .get_transactions_in_range(start, end)?
            .into_iter()
            .map(|(_, digest)| digest)
            .collect())
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<SuiTransactionResponse> {
        let (transaction, effects) = self
            .state
            .get_executed_transaction_and_effects(digest)
            .await
            .tap_err(|err| debug!(tx_digest=?digest, "Failed to get transaction: {:?}", err))?;
        let checkpoint = self
            .state
            .get_transaction_checkpoint(&digest)
            .map_err(|e| anyhow!("{e}"))?;
        let checkpoint_timestamp = checkpoint.as_ref().map(|c| c.summary.timestamp_ms);

        let events = if let Some(digest) = effects.events_digest() {
            let events = self
                .state
                .get_transaction_events(*digest)
                .await
                .map_err(Error::from)?;
            SuiTransactionEvents::try_from(
                events,
                // threading the epoch_store through this API does not
                // seem possible, so we just read it from the state and fetch
                // the module cache out of it.
                // Notice that no matter what module cache we get things
                // should work
                self.state
                    .load_epoch_store_one_call_per_task()
                    .module_cache()
                    .as_ref(),
            )?
        } else {
            SuiTransactionEvents::default()
        };

        Ok(SuiTransactionResponse {
            transaction: transaction.into_message().try_into()?,
            effects: effects.try_into()?,
            events,
            timestamp_ms: checkpoint_timestamp,
            confirmed_local_execution: None,
            checkpoint: checkpoint.map(|c| c.summary.sequence_number),
        })
    }

    async fn multi_get_transactions(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> RpcResult<Vec<SuiTransactionResponse>> {
        if digests.len() <= QUERY_MAX_RESULT_LIMIT {
            let mut tx_digests: Vec<TransactionDigest> = digests
                .iter()
                .take(QUERY_MAX_RESULT_LIMIT)
                .copied()
                .collect();
            tx_digests.dedup();

            let txn_batch = self
                .state
                .multi_get_transactions(&tx_digests)
                .await
                .tap_err(|err| debug!(txs_digests=?tx_digests, "Failed to get batch: {:?}", err))?;

            let mut responses: Vec<SuiTransactionResponse> = Vec::new();
            for (txn, digest) in txn_batch.into_iter().zip(tx_digests.iter()) {
                let (transaction, effects, events, checkpoint) = txn;
                responses.push(SuiTransactionResponse {
                    transaction: transaction.into_message().try_into()?,
                    effects: effects.try_into()?,
                    events: SuiTransactionEvents::try_from(
                        events,
                        // threading the epoch_store through this API does not
                        // seem possible, so we just read it from the state and fetch
                        // the module cache out of it.
                        // Notice that no matter what module cache we get things
                        // should work
                        self.state
                            .load_epoch_store_one_call_per_task()
                            .module_cache()
                            .as_ref(),
                    )?,
                    timestamp_ms: self.state.get_timestamp_ms(digest).await?,
                    confirmed_local_execution: None,
                    checkpoint: checkpoint.map(|(_epoch, checkpoint)| checkpoint),
                })
            }
            Ok(responses)
        } else {
            Err(anyhow!(UserInputError::SizeLimitExceeded {
                limit: "input limit".to_string(),
                value: QUERY_MAX_RESULT_LIMIT.to_string()
            })
            .into())
        }
    }

    async fn get_normalized_move_modules_by_package(
        &self,
        package: ObjectID,
    ) -> RpcResult<BTreeMap<String, SuiMoveNormalizedModule>> {
        let modules = get_move_modules_by_package(self, package).await?;
        Ok(modules
            .into_iter()
            .map(|(name, module)| (name, module.into()))
            .collect::<BTreeMap<String, SuiMoveNormalizedModule>>())
    }

    async fn get_normalized_move_module(
        &self,
        package: ObjectID,
        module_name: String,
    ) -> RpcResult<SuiMoveNormalizedModule> {
        let module = get_move_module(self, package, module_name).await?;
        Ok(module.into())
    }

    async fn get_normalized_move_struct(
        &self,
        package: ObjectID,
        module_name: String,
        struct_name: String,
    ) -> RpcResult<SuiMoveNormalizedStruct> {
        let module = get_move_module(self, package, module_name).await?;
        let structs = module.structs;
        let identifier = Identifier::new(struct_name.as_str()).map_err(|e| anyhow!("{e}"))?;
        Ok(match structs.get(&identifier) {
            Some(struct_) => Ok(struct_.clone().into()),
            None => Err(anyhow!(
                "No struct was found with struct name {}",
                struct_name
            )),
        }?)
    }

    async fn get_normalized_move_function(
        &self,
        package: ObjectID,
        module_name: String,
        function_name: String,
    ) -> RpcResult<SuiMoveNormalizedFunction> {
        let module = get_move_module(self, package, module_name).await?;
        let functions = module.exposed_functions;
        let identifier = Identifier::new(function_name.as_str()).map_err(|e| anyhow!("{e}"))?;
        Ok(match functions.get(&identifier) {
            Some(function) => Ok(function.clone().into()),
            None => Err(anyhow!(
                "No function was found with function name {}",
                function_name
            )),
        }?)
    }

    async fn get_move_function_arg_types(
        &self,
        package: ObjectID,
        module: String,
        function: String,
    ) -> RpcResult<Vec<MoveFunctionArgType>> {
        let object_read = self
            .state
            .get_object_read(&package)
            .await
            .map_err(|e| anyhow!("{e}"))?;

        let normalized = match object_read {
            ObjectRead::Exists(_obj_ref, object, _layout) => match object.data {
                Data::Package(p) => normalize_modules(p.serialized_module_map().values())
                    .map_err(|e| anyhow!("{e}")),
                _ => Err(anyhow!("Object is not a package with ID {}", package)),
            },
            _ => Err(anyhow!("Package object does not exist with ID {}", package)),
        }?;

        let identifier = Identifier::new(function.as_str()).map_err(|e| anyhow!("{e}"))?;
        let parameters = normalized.get(&module).and_then(|m| {
            m.exposed_functions
                .get(&identifier)
                .map(|f| f.parameters.clone())
        });

        Ok(match parameters {
            Some(parameters) => Ok(parameters
                .iter()
                .map(|p| match p {
                    Type::Struct {
                        address: _,
                        module: _,
                        name: _,
                        type_arguments: _,
                    } => MoveFunctionArgType::Object(ObjectValueKind::ByValue),
                    Type::Reference(_) => {
                        MoveFunctionArgType::Object(ObjectValueKind::ByImmutableReference)
                    }
                    Type::MutableReference(_) => {
                        MoveFunctionArgType::Object(ObjectValueKind::ByMutableReference)
                    }
                    _ => MoveFunctionArgType::Pure,
                })
                .collect::<Vec<MoveFunctionArgType>>()),
            None => Err(anyhow!("No parameters found for function {}", function)),
        }?)
    }

    async fn get_transactions(
        &self,
        query: TransactionQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionsPage> {
        let limit = cap_page_limit(limit);
        let descending = descending_order.unwrap_or_default();

        // Retrieve 1 extra item for next cursor
        let mut data = self
            .state
            .get_transactions(query, cursor, Some(limit + 1), descending)?;

        // extract next cursor
        let next_cursor = data.get(limit).cloned();
        data.truncate(limit);
        Ok(Page { data, next_cursor })
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<CheckpointSequenceNumber> {
        Ok(self
            .state
            .get_latest_checkpoint_sequence_number()
            .map_err(|e| {
                anyhow!("Latest checkpoint sequence number was not found with error :{e}")
            })?)
    }

    async fn get_checkpoint(&self, id: CheckpointId) -> RpcResult<Checkpoint> {
        Ok(self.get_checkpoint_internal(id)?)
    }

    async fn get_checkpoint_summary_by_digest(
        &self,
        digest: CheckpointDigest,
    ) -> RpcResult<CheckpointSummary> {
        Ok(self
            .state
            .get_checkpoint_summary_by_digest(digest)
            .map_err(|e| {
                anyhow!(
                    "Checkpoint summary based on digest: {digest:?} were not found with error: {e}"
                )
            })?)
    }

    async fn get_checkpoint_summary(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> RpcResult<CheckpointSummary> {
        Ok(self.state.get_checkpoint_summary_by_sequence_number(sequence_number)
            .map_err(|e| anyhow!("Checkpoint summary based on sequence number: {sequence_number} was not found with error :{e}"))?)
    }

    async fn get_checkpoint_contents_by_digest(
        &self,
        digest: CheckpointContentsDigest,
    ) -> RpcResult<CheckpointContents> {
        Ok(self.state.get_checkpoint_contents(digest).map_err(|e| {
            anyhow!(
                "Checkpoint contents based on digest: {digest:?} were not found with error: {e}"
            )
        })?)
    }

    async fn get_checkpoint_contents(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> RpcResult<CheckpointContents> {
        Ok(self
            .state
            .get_checkpoint_contents_by_sequence_number(sequence_number)
            .map_err(|e| anyhow!("Checkpoint contents based on seq number: {sequence_number} were not found with error: {e}"))?)
    }
}

impl SuiRpcModule for ReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::ReadApiOpenRpc::module_doc()
    }
}

async fn get_display_fields(
    fullnode_api: &ReadApi,
    original_object: &Object,
    original_layout: &Option<MoveStructLayout>,
) -> RpcResult<Option<BTreeMap<String, String>>> {
    let (object_type, layout) = get_object_type_and_struct(original_object, original_layout)?;
    if let Some(display_object) = get_display_object_by_type(fullnode_api, &object_type).await? {
        return Ok(Some(get_rendered_fields(display_object.fields, &layout)?));
    }
    Ok(None)
}

async fn get_display_object_by_type(
    fullnode_api: &ReadApi,
    object_type: &StructTag,
) -> RpcResult<Option<DisplayObject>> {
    let display_object_id = get_display_object_id(fullnode_api, object_type).await?;
    if display_object_id.is_none() {
        return Ok(None);
    }
    // safe to unwrap because `is_none` is checked above
    let display_object_id = display_object_id.unwrap();
    if let ObjectRead::Exists(_, display_object, _) = fullnode_api
        .state
        .get_object_read(&display_object_id)
        .await
        .map_err(|e| anyhow!("Failed to fetch display object {display_object_id}: {e}"))?
    {
        let move_object = display_object
            .data
            .try_as_move()
            .ok_or_else(|| anyhow!("Failed to extract Move object from {display_object_id}"))?;
        Ok(Some(
            bcs::from_bytes::<DisplayObject>(move_object.contents()).map_err(|e| {
                anyhow!("Failed to deserialize DisplayObject {display_object_id}: {e}")
            })?,
        ))
    } else {
        Err(anyhow!("Display object {display_object_id} does not exist"))?
    }
}

async fn get_display_object_id(
    fullnode_api: &ReadApi,
    object_type: &StructTag,
) -> RpcResult<Option<ObjectID>> {
    let display_created_event = fullnode_api
        .state
        .query_events(
            EventQuery::MoveEvent(DisplayCreatedEvent::type_(object_type).to_string()),
            /* cursor */ None,
            /* limit */ 1,
            /* descending */ false,
        )
        .await?;
    if display_created_event.is_empty() {
        return Ok(None);
    }
    if let SuiEvent::MoveEvent { bcs, .. } = display_created_event[0].clone().1.event {
        let display_object_id = bcs::from_bytes::<DisplayCreatedEvent>(&bcs)
            .map_err(|e| anyhow!("Failed to deserialize DisplayCreatedEvent: {e}"))?
            .id
            .bytes;
        Ok(Some(display_object_id))
    } else {
        Err(anyhow!("Failed to extract display object id from event"))?
    }
}

fn get_object_type_and_struct(
    o: &Object,
    layout: &Option<MoveStructLayout>,
) -> RpcResult<(StructTag, MoveStruct)> {
    let object_type = o
        .type_()
        .ok_or_else(|| anyhow!("Failed to extract object type"))?
        .clone();
    let move_struct = get_move_struct(o, layout)?;
    Ok((object_type, move_struct))
}

fn get_move_struct(o: &Object, layout: &Option<MoveStructLayout>) -> RpcResult<MoveStruct> {
    let layout = layout
        .as_ref()
        .ok_or_else(|| anyhow!("Failed to extract layout"))?;
    Ok(o.data
        .try_as_move()
        .ok_or_else(|| anyhow!("Failed to extract Move object"))?
        .to_move_struct(layout)
        .map_err(|err| anyhow!("{err}"))?)
}

pub async fn get_move_module(
    fullnode_api: &ReadApi,
    package: ObjectID,
    module_name: String,
) -> RpcResult<NormalizedModule> {
    let normalized = get_move_modules_by_package(fullnode_api, package).await?;
    Ok(match normalized.get(&module_name) {
        Some(module) => Ok(module.clone()),
        None => Err(anyhow!("No module found with module name {}", module_name)),
    }?)
}

pub async fn get_move_modules_by_package(
    fullnode_api: &ReadApi,
    package: ObjectID,
) -> RpcResult<BTreeMap<String, NormalizedModule>> {
    let object_read = fullnode_api
        .state
        .get_object_read(&package)
        .await
        .map_err(|e| anyhow!("{e}"))?;

    Ok(match object_read {
        ObjectRead::Exists(_obj_ref, object, _layout) => match object.data {
            Data::Package(p) => {
                normalize_modules(p.serialized_module_map().values()).map_err(|e| anyhow!("{e}"))
            }
            _ => Err(anyhow!("Object is not a package with ID {}", package)),
        },
        _ => Err(anyhow!("Package object does not exist with ID {}", package)),
    }?)
}

pub fn get_transaction_data_and_digest(
    tx_bytes: Base64,
) -> RpcResult<(TransactionData, TransactionDigest)> {
    let tx_data =
        bcs::from_bytes(&tx_bytes.to_vec().map_err(|e| anyhow!(e))?).map_err(|e| anyhow!(e))?;
    let intent_msg = IntentMessage::new(
        Intent {
            version: IntentVersion::V0,
            scope: IntentScope::TransactionData,
            app_id: AppId::Sui,
        },
        tx_data,
    );
    let txn_digest = TransactionDigest::new(sha3_hash(&intent_msg.value));
    Ok((intent_msg.value, txn_digest))
}

pub fn get_rendered_fields(
    fields: VecMap<String, String>,
    move_struct: &MoveStruct,
) -> RpcResult<BTreeMap<String, String>> {
    let sui_move_value: SuiMoveValue = MoveValue::Struct(move_struct.clone()).into();
    if let SuiMoveValue::Struct(move_struct) = sui_move_value {
        return fields
            .contents
            .iter()
            .map(|entry| match parse_template(&entry.value, &move_struct) {
                Ok(value) => Ok((entry.key.clone(), value)),
                Err(e) => Err(e),
            })
            .collect::<RpcResult<BTreeMap<_, _>>>();
    }
    Err(anyhow!("Failed to parse move struct"))?
}

fn parse_template(template: &str, move_struct: &SuiMoveStruct) -> RpcResult<String> {
    let mut output = template.to_string();
    let mut var_name = String::new();
    let mut in_braces = false;
    let mut escaped = false;

    for ch in template.chars() {
        match ch {
            '\\' => {
                escaped = true;
                continue;
            }
            '{' if !escaped => {
                in_braces = true;
                var_name.clear();
            }
            '}' if !escaped => {
                in_braces = false;
                let value = get_value_from_move_struct(move_struct, &var_name)?;
                output = output.replace(&format!("{{{}}}", var_name), &value.to_string());
            }
            _ if !escaped => {
                if in_braces {
                    var_name.push(ch);
                }
            }
            _ => {}
        }
        escaped = false;
    }

    Ok(output.replace('\\', ""))
}

fn get_value_from_move_struct(move_struct: &SuiMoveStruct, var_name: &str) -> RpcResult<String> {
    let parts: Vec<&str> = var_name.split('.').collect();
    if parts.is_empty() {
        return Err(anyhow!("Display template value cannot be empty"))?;
    }
    if parts.len() > MAX_DISPLAY_NESTED_LEVEL {
        return Err(anyhow!(
            "Display template value nested depth cannot exist {}",
            MAX_DISPLAY_NESTED_LEVEL
        ))?;
    }
    let mut current_value = &SuiMoveValue::Struct(move_struct.clone());
    // iterate over the parts and try to access the corresponding field
    for part in parts {
        match current_value {
            SuiMoveValue::Struct(move_struct) => {
                if let SuiMoveStruct::WithTypes { type_: _, fields }
                | SuiMoveStruct::WithFields(fields) = move_struct
                {
                    if let Some(value) = fields.get(part) {
                        current_value = value;
                    } else {
                        return Err(anyhow!(
                            "Field value {} cannot be found in struct",
                            var_name
                        ))?;
                    }
                } else {
                    return Err(anyhow!(
                        "Unexpected move struct type for field {}",
                        var_name
                    ))?;
                }
            }
            _ => return Err(anyhow!("Unexpected move value type for field {}", var_name))?,
        }
    }

    match current_value {
        SuiMoveValue::Option(move_option) => match move_option.as_ref() {
            Some(move_value) => Ok(move_value.to_string()),
            None => Ok("".to_string()),
        },
        SuiMoveValue::Vector(_) => Err(anyhow!(
            "Vector is not supported as a Display value {}",
            var_name
        ))?,

        _ => Ok(current_value.to_string()),
    }
}
