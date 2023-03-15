// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use futures::future::join_all;
use itertools::Itertools;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use linked_hash_map::LinkedHashMap;
use move_binary_format::normalized::{Module as NormalizedModule, Type};
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use move_core_types::value::{MoveStruct, MoveStructLayout, MoveValue};
use tap::TapFallible;
use tracing::debug;

use shared_crypto::intent::{AppId, Intent, IntentMessage, IntentScope, IntentVersion};
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{
    BalanceChange, BigInt, Checkpoint, CheckpointId, DynamicFieldPage, MoveFunctionArgType,
    ObjectChange, ObjectValueKind, ObjectsPage, Page, SuiGetPastObjectRequest,
    SuiMoveNormalizedFunction, SuiMoveNormalizedModule, SuiMoveNormalizedStruct, SuiMoveStruct,
    SuiMoveValue, SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse,
    SuiTransactionEvents, SuiTransactionResponse, SuiTransactionResponseOptions,
    SuiTransactionResponseQuery, TransactionsPage,
};
use sui_open_rpc::Module;
use sui_types::base_types::{
    ObjectID, SequenceNumber, SuiAddress, TransactionDigest, TxSequenceNumber,
};
use sui_types::collection_types::VecMap;
use sui_types::crypto::default_hash;
use sui_types::digests::TransactionEventsDigest;
use sui_types::display::{DisplayCreatedEvent, DisplayObject};
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::error::UserInputError;
use sui_types::event::Event;
use sui_types::messages::TransactionDataAPI;
use sui_types::messages::{
    TransactionData, TransactionEffects, TransactionEffectsAPI, TransactionEvents,
    VerifiedTransaction,
};
use sui_types::messages_checkpoint::{CheckpointSequenceNumber, CheckpointTimestamp};
use sui_types::move_package::normalize_modules;
use sui_types::object::{Data, Object, ObjectRead, PastObjectRead};

use crate::api::ReadApiServer;
use crate::api::QUERY_MAX_RESULT_LIMIT;
use crate::api::{cap_page_limit, cap_page_objects_limit};
use crate::error::Error;
use crate::{
    get_balance_change_from_effect, get_object_change_from_effect, ObjectProviderCache,
    SuiRpcModule,
};

const MAX_DISPLAY_NESTED_LEVEL: usize = 10;

// An implementation of the read portion of the JSON-RPC interface intended for use in
// Fullnodes.
pub struct ReadApi {
    pub state: Arc<AuthorityState>,
}

// Internal data structure to make it easy to work with data returned from
// authority store and also enable code sharing between get_transaction_with_options,
// multi_get_transaction_with_options, etc.
#[derive(Default)]
struct IntermediateTransactionResponse {
    digest: TransactionDigest,
    transaction: Option<VerifiedTransaction>,
    effects: Option<TransactionEffects>,
    events: Option<SuiTransactionEvents>,
    checkpoint_seq: Option<CheckpointSequenceNumber>,
    balance_changes: Option<Vec<BalanceChange>>,
    object_changes: Option<Vec<ObjectChange>>,
    timestamp: Option<CheckpointTimestamp>,
    errors: Vec<String>,
}

impl IntermediateTransactionResponse {
    pub fn new(digest: TransactionDigest) -> Self {
        Self {
            digest,
            ..Default::default()
        }
    }
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
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        // exclusive cursor if `Some`, otherwise start from the beginning
        options: Option<SuiObjectDataOptions>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
        at_checkpoint: Option<CheckpointId>,
    ) -> RpcResult<ObjectsPage> {
        if at_checkpoint.is_some() {
            return Err(anyhow!("at_checkpoint param currently not supported").into());
        }
        let limit = cap_page_objects_limit(limit)?;
        let options = options.unwrap_or_default();

        // MUSTFIXD(jian): multi-get-object for content/storage rebate if opt.show_content is true
        let mut objects = self
            .state
            .get_owner_objects(address, cursor, limit + 1)
            .map_err(|e| anyhow!("{e}"))?;

        // objects here are of size (limit + 1), where the last one is the cursor for the next page
        let has_next_page = objects.len() > limit;
        objects.truncate(limit);
        let next_cursor = objects
            .last()
            .cloned()
            .map_or(cursor, |o_info| Some(o_info.object_id));

        let data = objects.into_iter().try_fold(vec![], |mut acc, o_info| {
            let o_resp = SuiObjectResponse::try_from((o_info, options.clone()))?;
            acc.push(o_resp);
            Ok::<Vec<SuiObjectResponse>, Error>(acc)
        })?;

        Ok(Page {
            data,
            next_cursor,
            has_next_page,
        })
    }

    async fn get_dynamic_fields(
        &self,
        parent_object_id: ObjectID,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<DynamicFieldPage> {
        let limit = cap_page_limit(limit);
        let mut data = self
            .state
            .get_dynamic_fields(parent_object_id, cursor, limit + 1)
            .map_err(|e| anyhow!("{e}"))?;
        let has_next_page = data.len() > limit;
        data.truncate(limit);
        let next_cursor = data.last().cloned().map_or(cursor, |c| Some(c.object_id));
        Ok(DynamicFieldPage {
            data,
            next_cursor,
            has_next_page,
        })
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

    async fn multi_get_object_with_options(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>> {
        if object_ids.len() <= QUERY_MAX_RESULT_LIMIT {
            let mut futures = vec![];
            for object_id in object_ids {
                futures.push(self.get_object_with_options(object_id, options.clone()))
            }
            let results = join_all(futures).await;
            let (oks, errs): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);

            let success = oks.into_iter().filter_map(Result::ok).collect();
            let errors: Vec<_> = errs.into_iter().filter_map(Result::err).collect();
            if !errors.is_empty() {
                let error_string = errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<String>>()
                    .join("; ");
                Err(anyhow!("{error_string}").into())
            } else {
                Ok(success)
            }
        } else {
            Err(anyhow!(UserInputError::SizeLimitExceeded {
                limit: "input limit".to_string(),
                value: QUERY_MAX_RESULT_LIMIT.to_string()
            })
            .into())
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

    async fn try_multi_get_past_objects(
        &self,
        past_objects: Vec<SuiGetPastObjectRequest>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>> {
        if past_objects.len() <= QUERY_MAX_RESULT_LIMIT {
            let mut futures = vec![];
            for past_object in past_objects {
                futures.push(self.try_get_past_object(
                    past_object.object_id,
                    past_object.version,
                    options.clone(),
                ));
            }
            let results = join_all(futures).await;
            let (oks, errs): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);
            let success = oks.into_iter().filter_map(Result::ok).collect();
            let errors: Vec<_> = errs.into_iter().filter_map(Result::err).collect();
            if !errors.is_empty() {
                let error_string = errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<String>>()
                    .join("; ");
                Err(anyhow!("{error_string}").into())
            } else {
                Ok(success)
            }
        } else {
            Err(anyhow!(UserInputError::SizeLimitExceeded {
                limit: "input limit".to_string(),
                value: QUERY_MAX_RESULT_LIMIT.to_string()
            })
            .into())
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

    async fn get_total_transaction_number(&self) -> RpcResult<BigInt> {
        Ok(self.state.get_total_transaction_number()?.into())
    }

    async fn get_transactions_in_range_deprecated(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> RpcResult<Vec<TransactionDigest>> {
        Ok(self
            .state
            .get_transactions_in_range_deprecated(start, end)?
            .into_iter()
            .map(|(_, digest)| digest)
            .collect())
    }

    async fn get_transaction_with_options(
        &self,
        digest: TransactionDigest,
        opts: Option<SuiTransactionResponseOptions>,
    ) -> RpcResult<SuiTransactionResponse> {
        let opts = opts.unwrap_or_default();
        let mut temp_response = IntermediateTransactionResponse::new(digest);

        // the input is needed for object_changes to retrieve the sender address.
        if opts.show_input || opts.show_object_changes {
            temp_response.transaction =
                Some(self.state.get_executed_transaction(digest).await.tap_err(
                    |err| debug!(tx_digest=?digest, "Failed to get transaction: {:?}", err),
                )?);
        }

        // Fetch effects when `show_events` is true because events relies on effects
        if opts.require_effects() {
            temp_response.effects =
                Some(self.state.get_executed_effects(digest).await.tap_err(
                    |err| debug!(tx_digest=?digest, "Failed to get effects: {:?}", err),
                )?);
        }

        if let Some((_, seq)) = self
            .state
            .get_transaction_checkpoint_sequence(&digest)
            .map_err(|e| anyhow!("{e}"))?
        {
            temp_response.checkpoint_seq = Some(seq);
        }

        if temp_response.checkpoint_seq.is_some() {
            let checkpoint = self
                .state
                // safe to unwrap because we have checked `is_some` above
                .get_checkpoint_by_sequence_number(temp_response.checkpoint_seq.unwrap())
                .map_err(|e| anyhow!("{e}"))?;
            // TODO(chris): we don't need to fetch the whole checkpoint summary
            temp_response.timestamp = checkpoint.as_ref().map(|c| c.timestamp_ms);
        }

        if opts.show_events && temp_response.effects.is_some() {
            // safe to unwrap because we have checked is_some
            if let Some(event_digest) = temp_response.effects.as_ref().unwrap().events_digest() {
                let events = self
                    .state
                    .get_transaction_events(event_digest)
                    .map_err(Error::from)?;
                match to_sui_transaction_events(self, digest, events) {
                    Ok(e) => temp_response.events = Some(e),
                    Err(e) => temp_response.errors.push(e.to_string()),
                };
            } else {
                // events field will be Some if and only if `show_events` is true and
                // there is no error in converting fetching events
                temp_response.events = Some(SuiTransactionEvents::default());
            }
        }

        let object_cache = ObjectProviderCache::new(self.state.clone());
        if opts.show_balance_changes {
            if let Some(effects) = &temp_response.effects {
                let balance_changes = get_balance_change_from_effect(&object_cache, effects)
                    .await
                    .map_err(Error::SuiError)?;
                temp_response.balance_changes = Some(balance_changes);
            }
        }

        if opts.show_object_changes {
            if let (Some(effects), Some(input)) =
                (&temp_response.effects, &temp_response.transaction)
            {
                let sender = input.data().intent_message().value.sender();
                let object_changes = get_object_change_from_effect(&object_cache, sender, effects)
                    .await
                    .map_err(Error::SuiError)?;
                temp_response.object_changes = Some(object_changes);
            }
        }

        Ok(convert_to_response(temp_response, &opts))
    }

    async fn multi_get_transactions_with_options(
        &self,
        digests: Vec<TransactionDigest>,
        opts: Option<SuiTransactionResponseOptions>,
    ) -> RpcResult<Vec<SuiTransactionResponse>> {
        let num_digests = digests.len();
        if num_digests > QUERY_MAX_RESULT_LIMIT {
            return Err(anyhow!(UserInputError::SizeLimitExceeded {
                limit: "multi get transaction input limit".to_string(),
                value: QUERY_MAX_RESULT_LIMIT.to_string()
            })
            .into());
        }

        let opts = opts.unwrap_or_default();
        if opts.show_balance_changes || opts.show_object_changes {
            // Not supported because it's likely the response will easily exceed response limit
            return Err(anyhow!(UserInputError::Unsupported(
                "show_balance_changes and show_object_changes is not available on \
                multiGetTransactions"
                    .to_string()
            ))
            .into());
        }

        // use LinkedHashMap to dedup and can iterate in insertion order.
        let mut temp_response: LinkedHashMap<&TransactionDigest, IntermediateTransactionResponse> =
            LinkedHashMap::from_iter(
                digests
                    .iter()
                    .map(|k| (k, IntermediateTransactionResponse::new(*k))),
            );
        if temp_response.len() < num_digests {
            return Err(anyhow!("The list of digests in the input contain duplicates").into());
        }

        if opts.show_input {
            let transactions = self
                .state
                .multi_get_executed_transactions(&digests)
                .await
                .tap_err(
                    |err| debug!(digests=?digests, "Failed to multi get transaction: {:?}", err),
                )?;

            for ((_digest, cache_entry), txn) in
                temp_response.iter_mut().zip(transactions.into_iter())
            {
                cache_entry.transaction = txn;
            }
        }

        // Fetch effects when `show_events` is true because events relies on effects
        if opts.show_effects || opts.show_events {
            let effects_list = self
                .state
                .multi_get_executed_effects(&digests)
                .await
                .tap_err(
                    |err| debug!(digests=?digests, "Failed to multi get effects: {:?}", err),
                )?;
            for ((_digest, cache_entry), e) in
                temp_response.iter_mut().zip(effects_list.into_iter())
            {
                cache_entry.effects = e;
            }
        }

        let checkpoint_seq_list = self
                .state
                .multi_get_transaction_checkpoint(&digests)
                .await
                .tap_err(
                    |err| debug!(digests=?digests, "Failed to multi get checkpoint sequence number: {:?}", err))?;
        for ((_digest, cache_entry), seq) in temp_response
            .iter_mut()
            .zip(checkpoint_seq_list.into_iter())
        {
            cache_entry.checkpoint_seq = seq.map(|(_, seq)| seq);
        }

        let unique_checkpoint_numbers = temp_response
            .values()
            .filter_map(|cache_entry| cache_entry.checkpoint_seq)
            // It's likely that many transactions have the same checkpoint, so we don't
            // need to over-fetch
            .unique()
            .collect::<Vec<CheckpointSequenceNumber>>();

        // fetch timestamp from the DB
        let timestamps = self
            .state
            .multi_get_checkpoint_by_sequence_number(&unique_checkpoint_numbers)
            .map_err(|e| anyhow!("{e}"))?
            .into_iter()
            .map(|c| c.map(|checkpoint| checkpoint.timestamp_ms));

        // construct a hashmap of checkpoint -> timestamp for fast lookup
        let checkpoint_to_timestamp = unique_checkpoint_numbers
            .into_iter()
            .zip(timestamps)
            .collect::<HashMap<_, _>>();

        // fill cache with the timestamp
        for (_, cache_entry) in temp_response.iter_mut() {
            if cache_entry.checkpoint_seq.is_some() {
                // safe to unwrap because is_some is checked
                cache_entry.timestamp = *checkpoint_to_timestamp
                    .get(cache_entry.checkpoint_seq.as_ref().unwrap())
                    // Safe to unwrap because checkpoint_seq is guaranteed to exist in checkpoint_to_timestamp
                    .unwrap();
            }
        }

        if opts.show_events {
            let event_digests_list = temp_response
                .values()
                .filter_map(|cache_entry| match &cache_entry.effects {
                    Some(eff) => eff.events_digest().cloned(),
                    None => None,
                })
                .collect::<Vec<TransactionEventsDigest>>();

            // fetch events from the DB
            let events = self
                .state
                .multi_get_events(&event_digests_list)
                .map_err(|e| anyhow!("{e}"))?
                .into_iter();

            // construct a hashmap of event digests -> events for fast lookup
            let mut event_digest_to_events = event_digests_list
                .into_iter()
                .zip(events)
                .collect::<HashMap<_, _>>();

            // fill cache with the events
            for (_, cache_entry) in temp_response.iter_mut() {
                let event_digest: Option<Option<TransactionEventsDigest>> = cache_entry
                    .effects
                    .as_ref()
                    .map(|e| e.events_digest().cloned());
                let event_digest = event_digest.flatten();
                if event_digest.is_some() {
                    // safe to unwrap because `is_some` is checked
                    let events: Option<RpcResult<SuiTransactionEvents>> = event_digest_to_events
                        .remove(event_digest.as_ref().unwrap())
                        .expect("This can only happen if there are two or more transaction digests sharing the same event digests, which should never happen")
                        .map(|e| to_sui_transaction_events(self, cache_entry.digest, e));
                    match events {
                        Some(Ok(e)) => cache_entry.events = Some(e),
                        Some(Err(e)) => cache_entry.errors.push(e.to_string()),
                        None => cache_entry.errors.push(format!(
                            "Failed to fetch events with event digest {:?}",
                            event_digest.unwrap()
                        )),
                    }
                } else {
                    // events field will be Some if and only if `show_events` is true and
                    // there is no error in converting fetching events
                    cache_entry.events = Some(SuiTransactionEvents::default());
                }
            }
        }

        Ok(temp_response
            .into_iter()
            .map(|c| convert_to_response(c.1, &opts))
            .collect::<Vec<_>>())
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

    async fn query_transactions(
        &self,
        query: SuiTransactionResponseQuery,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionsPage> {
        let limit = cap_page_limit(limit);
        let descending = descending_order.unwrap_or_default();
        let opts = query.options.unwrap_or_default();
        if opts.show_balance_changes || opts.show_object_changes {
            // Not supported because it's likely the response will easily exceed response limit
            return Err(anyhow!(UserInputError::Unsupported(
                "show_balance_changes and show_object_changes is not available on \
                queryTransactions"
                    .to_string()
            ))
            .into());
        }

        // Retrieve 1 extra item for next cursor
        let mut digests =
            self.state
                .get_transactions(query.filter, cursor, Some(limit + 1), descending)?;

        // extract next cursor
        let has_next_page = digests.len() > limit;
        digests.truncate(limit);
        let next_cursor = digests.last().cloned().map_or(cursor, Some);

        let data: Vec<SuiTransactionResponse> = if opts.only_digest() {
            digests
                .into_iter()
                .map(SuiTransactionResponse::new)
                .collect()
        } else {
            self.multi_get_transactions_with_options(digests, Some(opts))
                .await?
        };

        Ok(Page {
            data,
            next_cursor,
            has_next_page,
        })
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
}

impl SuiRpcModule for ReadApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::ReadApiOpenRpc::module_doc()
    }
}

fn to_sui_transaction_events(
    fullnode_api: &ReadApi,
    tx_digest: TransactionDigest,
    events: TransactionEvents,
) -> RpcResult<SuiTransactionEvents> {
    Ok(SuiTransactionEvents::try_from(
        events,
        tx_digest,
        None,
        // threading the epoch_store through this API does not
        // seem possible, so we just read it from the state and fetch
        // the module cache out of it.
        // Notice that no matter what module cache we get things
        // should work
        fullnode_api
            .state
            .load_epoch_store_one_call_per_task()
            .module_cache()
            .as_ref(),
    )
    .map_err(Error::SuiError)?)
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
) -> Result<Option<ObjectID>, Error> {
    let package_id = ObjectID::from(object_type.address);
    let package_tx = fullnode_api
        .state
        .get_object_read(&package_id)
        .await?
        .into_object()?
        .previous_transaction;
    let effects = fullnode_api.state.get_executed_effects(package_tx).await?;
    let Some(event_digest) = effects.events_digest() else {
        return Ok(None);
    };
    let events = fullnode_api.state.get_transaction_events(event_digest)?;
    let Some(display_created_event) = events.data.iter().find(|e|{
        e.type_ == DisplayCreatedEvent::type_(object_type)
    }) else{
        return Ok(None);
    };
    let Event { contents, .. } = display_created_event;
    let display_object_id = bcs::from_bytes::<DisplayCreatedEvent>(contents)
        .map_err(|e| anyhow!("Failed to deserialize DisplayCreatedEvent: {e}"))?
        .id
        .bytes;
    Ok(Some(display_object_id))
}

fn get_object_type_and_struct(
    o: &Object,
    layout: &Option<MoveStructLayout>,
) -> RpcResult<(StructTag, MoveStruct)> {
    let object_type = o
        .type_()
        .ok_or_else(|| anyhow!("Failed to extract object type"))?
        .clone()
        .into();
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
    let txn_digest = TransactionDigest::new(default_hash(&intent_msg.value));
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

fn convert_to_response(
    cache: IntermediateTransactionResponse,
    opts: &SuiTransactionResponseOptions,
) -> SuiTransactionResponse {
    let mut response = SuiTransactionResponse::new(cache.digest);
    response.errors = cache.errors;

    if opts.show_input && cache.transaction.is_some() {
        match cache.transaction.unwrap().into_message().try_into() {
            Ok(t) => {
                response.transaction = Some(t);
            }
            Err(e) => {
                response.errors.push(e.to_string());
            }
        }
    }

    if opts.show_effects && cache.effects.is_some() {
        match cache.effects.unwrap().try_into() {
            Ok(effects) => {
                response.effects = Some(effects);
            }
            Err(e) => {
                response.errors.push(e.to_string());
            }
        }
    }

    response.checkpoint = cache.checkpoint_seq;
    response.timestamp_ms = cache.timestamp;

    if opts.show_events {
        response.events = cache.events;
    }

    if opts.show_balance_changes {
        response.balance_changes = cache.balance_changes;
    }

    if opts.show_object_changes {
        response.object_changes = cache.object_changes;
    }
    response
}
