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
use move_binary_format::{file_format_common::VERSION_MAX, normalized::Module as NormalizedModule};
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::StructTag;
use move_core_types::value::{MoveStruct, MoveStructLayout, MoveValue};
use tap::TapFallible;
use tracing::{debug, error, info, instrument, warn};

use mysten_metrics::spawn_monitored_task;
use shared_crypto::intent::{AppId, Intent, IntentMessage, IntentScope, IntentVersion};
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{
    BalanceChange, Checkpoint, CheckpointId, CheckpointPage, DisplayFieldsResponse, EventFilter,
    ObjectChange, SuiEvent, SuiGetPastObjectRequest, SuiMoveStruct, SuiMoveValue,
    SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse, SuiTransactionBlock,
    SuiTransactionBlockEvents, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_json_rpc_types::{SuiLoadedChildObject, SuiLoadedChildObjectsResponse};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SequenceNumber, TransactionDigest};
use sui_types::collection_types::VecMap;
use sui_types::crypto::default_hash;
use sui_types::digests::TransactionEventsDigest;
use sui_types::display::DisplayVersionUpdatedEvent;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::error::{SuiObjectResponseError, UserInputError};
use sui_types::messages::TransactionDataAPI;
use sui_types::messages::{TransactionData, VerifiedTransaction};
use sui_types::messages_checkpoint::{CheckpointSequenceNumber, CheckpointTimestamp};
use sui_types::move_package::normalize_modules;
use sui_types::object::{Data, Object, ObjectRead, PastObjectRead};
use sui_types::sui_serde::BigInt;

use crate::api::JsonRpcMetrics;
use crate::api::{validate_limit, ReadApiServer};
use crate::api::{QUERY_MAX_RESULT_LIMIT, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS};
use crate::error::Error;
use crate::with_tracing;
use crate::{
    get_balance_changes_from_effect, get_object_changes, ObjectProviderCache, SuiRpcModule,
};

const MAX_DISPLAY_NESTED_LEVEL: usize = 10;

// An implementation of the read portion of the JSON-RPC interface intended for use in
// Fullnodes.
#[derive(Clone)]
pub struct ReadApi {
    pub state: Arc<AuthorityState>,
    pub metrics: Arc<JsonRpcMetrics>,
}

// Internal data structure to make it easy to work with data returned from
// authority store and also enable code sharing between get_transaction_with_options,
// multi_get_transaction_with_options, etc.
#[derive(Default)]
struct IntermediateTransactionResponse {
    digest: TransactionDigest,
    transaction: Option<VerifiedTransaction>,
    effects: Option<TransactionEffects>,
    events: Option<SuiTransactionBlockEvents>,
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

    pub fn transaction(&self) -> &Option<VerifiedTransaction> {
        &self.transaction
    }
}

impl ReadApi {
    pub fn new(state: Arc<AuthorityState>, metrics: Arc<JsonRpcMetrics>) -> Self {
        Self { state, metrics }
    }

    fn get_checkpoint_internal(&self, id: CheckpointId) -> Result<Checkpoint, Error> {
        Ok(match id {
            CheckpointId::SequenceNumber(seq) => {
                let verified_summary =
                    self.state.get_verified_checkpoint_by_sequence_number(seq)?;
                let content = self
                    .state
                    .get_checkpoint_contents(verified_summary.content_digest)?;
                let signature = verified_summary.auth_sig().signature.clone();
                (
                    verified_summary.into_inner().into_data(),
                    content,
                    signature,
                )
                    .into()
            }
            CheckpointId::Digest(digest) => {
                let verified_summary = self
                    .state
                    .get_verified_checkpoint_summary_by_digest(digest)?;
                let content = self
                    .state
                    .get_checkpoint_contents(verified_summary.content_digest)?;
                let signature = verified_summary.auth_sig().signature.clone();
                (
                    verified_summary.into_inner().into_data(),
                    content,
                    signature,
                )
                    .into()
            }
        })
    }

    async fn multi_get_transaction_blocks_internal(
        &self,
        digests: Vec<TransactionDigest>,
        opts: Option<SuiTransactionBlockResponseOptions>,
    ) -> Result<Vec<SuiTransactionBlockResponse>, Error> {
        let num_digests = digests.len();
        if num_digests > *QUERY_MAX_RESULT_LIMIT {
            return Err(anyhow!(UserInputError::SizeLimitExceeded {
                limit: "multi get transaction input limit".to_string(),
                value: QUERY_MAX_RESULT_LIMIT.to_string()
            })
            .into());
        }
        self.metrics
            .get_tx_blocks_limit
            .report(digests.len() as u64);

        let opts = opts.unwrap_or_default();

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

        if opts.require_input() {
            let state = self.state.clone();
            let digest_clone = digests.clone();
            let transactions =
                state.multi_get_executed_transactions(&digest_clone).tap_err(
                    |err| debug!(digests=?digest_clone, "Failed to multi get transaction: {:?}", err),
                )?;

            for ((_digest, cache_entry), txn) in
                temp_response.iter_mut().zip(transactions.into_iter())
            {
                cache_entry.transaction = txn;
            }
        }

        // Fetch effects when `show_events` is true because events relies on effects
        if opts.require_effects() {
            let state = self.state.clone();
            let digests_clone = digests.clone();
            let effects_list = state.multi_get_executed_effects(&digests_clone).tap_err(
                |err| debug!(digests=?digests_clone, "Failed to multi get effects: {:?}", err),
            )?;
            for ((_digest, cache_entry), e) in
                temp_response.iter_mut().zip(effects_list.into_iter())
            {
                cache_entry.effects = e;
            }
        }

        let state = self.state.clone();
        let digests_clone = digests.clone();
        let checkpoint_seq_list =
            state
            .multi_get_transaction_checkpoint(&digests_clone)
            .tap_err(
                |err| debug!(digests=?digests_clone, "Failed to multi get checkpoint sequence number: {:?}", err))?;
        for ((_digest, cache_entry), seq) in temp_response
            .iter_mut()
            .zip(checkpoint_seq_list.into_iter())
        {
            cache_entry.checkpoint_seq = seq.map(|(_, seq)| seq);
        }

        let unique_checkpoint_numbers = temp_response
            .values()
            .filter_map(|cache_entry| cache_entry.checkpoint_seq.map(<u64>::from))
            // It's likely that many transactions have the same checkpoint, so we don't
            // need to over-fetch
            .unique()
            .collect::<Vec<CheckpointSequenceNumber>>();

        // fetch timestamp from the DB
        let timestamps = self
            .state
            .multi_get_checkpoint_by_sequence_number(&unique_checkpoint_numbers)
            .map_err(|e| {
                error!("Failed to fetch checkpoint summarys by these checkpoint ids: {unique_checkpoint_numbers:?} with error: {e:?}");
                anyhow!("{e}")
            })?
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
                    .get(
                        cache_entry
                            .checkpoint_seq
                            .map(<u64>::from)
                            .as_ref()
                            .unwrap(),
                    )
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
                .map_err(|e| {
                    error!("Failed to call multi_get_events for transactions {digests:?} with event digests {event_digests_list:?}");
                    anyhow!("{e}")
                })?
                .into_iter();

            // construct a hashmap of event digests -> events for fast lookup
            let event_digest_to_events = event_digests_list
                .into_iter()
                .zip(events)
                .collect::<HashMap<_, _>>();

            // fill cache with the events
            for (_, cache_entry) in temp_response.iter_mut() {
                let transaction_digest = cache_entry.digest;
                let event_digest: Option<Option<TransactionEventsDigest>> = cache_entry
                    .effects
                    .as_ref()
                    .map(|e| e.events_digest().cloned());
                let event_digest = event_digest.flatten();
                if event_digest.is_some() {
                    // safe to unwrap because `is_some` is checked
                    let event_digest = event_digest.as_ref().unwrap();
                    let events: Option<RpcResult<SuiTransactionBlockEvents>> = event_digest_to_events
                        .get(event_digest)
                        .cloned()
                        .unwrap_or_else(|| panic!("Expect event digest {event_digest:?} to be found in cache for transaction {transaction_digest}"))
                        .map(|events| to_sui_transaction_events(self, cache_entry.digest, events));
                    match events {
                        Some(Ok(e)) => cache_entry.events = Some(e),
                        Some(Err(e)) => cache_entry.errors.push(e.to_string()),
                        None => {
                            error!("Failed to fetch events with event digest {event_digest:?} for txn {transaction_digest}");
                            cache_entry.errors.push(format!(
                                "Failed to fetch events with event digest {event_digest:?}",
                            ))
                        }
                    }
                } else {
                    // events field will be Some if and only if `show_events` is true and
                    // there is no error in converting fetching events
                    cache_entry.events = Some(SuiTransactionBlockEvents::default());
                }
            }
        }

        let object_cache = ObjectProviderCache::new(self.state.clone());
        if opts.show_balance_changes {
            let mut results = vec![];
            for resp in temp_response.values() {
                let input_objects = if let Some(tx) = resp.transaction() {
                    tx.data()
                        .inner()
                        .intent_message
                        .value
                        .input_objects()
                        .unwrap_or_default()
                } else {
                    // don't have the input tx, so not much we can do. perhaps this is an Err?
                    Vec::new()
                };
                results.push(get_balance_changes_from_effect(
                    &object_cache,
                    resp.effects.as_ref().ok_or_else(|| {
                        anyhow!("unable to derive balance changes because effect is empty")
                    })?,
                    input_objects,
                    None,
                ));
            }
            let results = join_all(results).await;
            for (result, entry) in results.into_iter().zip(temp_response.iter_mut()) {
                match result {
                    Ok(balance_changes) => entry.1.balance_changes = Some(balance_changes),
                    Err(e) => entry
                        .1
                        .errors
                        .push(format!("Failed to fetch balance changes {e:?}")),
                }
            }
        }

        if opts.show_object_changes {
            let mut results = vec![];
            for resp in temp_response.values() {
                let effects = resp.effects.as_ref().ok_or_else(|| {
                    anyhow!("unable to derive object changes because effect is empty")
                })?;

                results.push(get_object_changes(
                    &object_cache,
                    resp.transaction
                        .as_ref()
                        .ok_or_else(|| {
                            anyhow!("unable to derive object changes because effect is empty")
                        })?
                        .data()
                        .intent_message()
                        .value
                        .sender(),
                    effects.modified_at_versions(),
                    effects.all_changed_objects(),
                    effects.all_deleted(),
                ));
            }
            let results = join_all(results).await;
            for (result, entry) in results.into_iter().zip(temp_response.iter_mut()) {
                match result {
                    Ok(object_changes) => entry.1.object_changes = Some(object_changes),
                    Err(e) => entry
                        .1
                        .errors
                        .push(format!("Failed to fetch object changes {e:?}")),
                }
            }
        }

        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        let converted_tx_block_resps = temp_response
            .into_iter()
            .map(|c| convert_to_response(c.1, &opts, epoch_store.module_cache()))
            .collect::<Result<Vec<_>, _>>()?;

        self.metrics
            .get_tx_blocks_result_size
            .report(converted_tx_block_resps.len() as u64);
        self.metrics
            .get_tx_blocks_result_size_total
            .inc_by(converted_tx_block_resps.len() as u64);
        Ok(converted_tx_block_resps)
    }
}

#[async_trait]
impl ReadApiServer for ReadApi {
    #[instrument(skip(self))]
    async fn get_object(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        with_tracing!("get_object", async move {
            let state = self.state.clone();
            let object_read = spawn_monitored_task!(async move {
                state.get_object_read(&object_id).map_err(|e| {
                    warn!(?object_id, "Failed to get object: {:?}", e);
                    anyhow!("{e}")
                })
            })
            .await
            .map_err(|e| anyhow!(e))??;
            let options = options.unwrap_or_default();

            match object_read {
                ObjectRead::NotExists(id) => Ok(SuiObjectResponse::new_with_error(
                    SuiObjectResponseError::NotExists { object_id: id },
                )),
                ObjectRead::Exists(object_ref, o, layout) => {
                    let mut display_fields = None;
                    if options.show_display {
                        match get_display_fields(self, &o, &layout) {
                            Ok(rendered_fields) => display_fields = Some(rendered_fields),
                            Err(e) => {
                                return Ok(SuiObjectResponse::new(
                                    Some((object_ref, o, layout, options, None).try_into()?),
                                    Some(SuiObjectResponseError::DisplayError {
                                        error: e.to_string(),
                                    }),
                                ));
                            }
                        }
                    }
                    Ok(SuiObjectResponse::new_with_data(
                        (object_ref, o, layout, options, display_fields).try_into()?,
                    ))
                }
                ObjectRead::Deleted((object_id, version, digest)) => Ok(
                    SuiObjectResponse::new_with_error(SuiObjectResponseError::Deleted {
                        object_id,
                        version,
                        digest,
                    }),
                ),
            }
        })
    }

    #[instrument(skip(self))]
    async fn multi_get_objects(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>> {
        with_tracing!("multi_get_objects", async move {
            if object_ids.len() <= *QUERY_MAX_RESULT_LIMIT {
                self.metrics
                    .get_objects_limit
                    .report(object_ids.len() as u64);
                let mut futures = vec![];
                for object_id in object_ids {
                    futures.push(self.get_object(object_id, options.clone()));
                }
                let results = join_all(futures).await;

                let objects_result: Result<Vec<SuiObjectResponse>, String> = results
                    .into_iter()
                    .map(|result| match result {
                        Ok(response) => Ok(response),
                        Err(error) => {
                            error!("Failed to fetch object with error: {error:?}");
                            Err(format!("Error: {}", error))
                        }
                    })
                    .collect();

                let objects = objects_result.map_err(|err| {
                    Error::UnexpectedError(format!("Failed to fetch objects with error: {}", err))
                })?;

                self.metrics
                    .get_objects_result_size
                    .report(objects.len() as u64);
                self.metrics
                    .get_objects_result_size_total
                    .inc_by(objects.len() as u64);
                Ok(objects)
            } else {
                Err(anyhow!(UserInputError::SizeLimitExceeded {
                    limit: "input limit".to_string(),
                    value: QUERY_MAX_RESULT_LIMIT.to_string()
                })
                .into())
            }
        })
    }

    #[instrument(skip(self))]
    async fn try_get_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse> {
        with_tracing!("try_get_past_object", async move {
            let state = self.state.clone();
            let past_read = spawn_monitored_task!(async move {
            state.get_past_object_read(&object_id, version)
            .map_err(|e| {
                error!("Failed to call try_get_past_object for object: {object_id:?} version: {version:?} with error: {e:?}");
                anyhow!("{e}")
            })}).await.map_err(|e| anyhow!(e))??;
            let options = options.unwrap_or_default();
            match past_read {
                PastObjectRead::ObjectNotExists(id) => {
                    Ok(SuiPastObjectResponse::ObjectNotExists(id))
                }
                PastObjectRead::VersionFound(object_ref, o, layout) => {
                    let display_fields = if options.show_display {
                        // TODO (jian): api breaking change to also modify past objects.
                        Some(get_display_fields(self, &o, &layout)?)
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
        })
    }

    #[instrument(skip(self))]
    async fn try_multi_get_past_objects(
        &self,
        past_objects: Vec<SuiGetPastObjectRequest>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>> {
        with_tracing!("try_multi_get_past_objects", async move {
            if past_objects.len() <= *QUERY_MAX_RESULT_LIMIT {
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
        })
    }

    #[instrument(skip(self))]
    async fn get_total_transaction_blocks(&self) -> RpcResult<BigInt<u64>> {
        with_tracing!("get_total_transaction_blocks", async move {
            Ok(self.state.get_total_transaction_blocks()?.into())
        })
    }

    #[instrument(skip(self))]
    async fn get_transaction_block(
        &self,
        digest: TransactionDigest,
        opts: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        with_tracing!("get_transaction_block", async move {
            let opts = opts.unwrap_or_default();
            let mut temp_response = IntermediateTransactionResponse::new(digest);

            // Fetch transaction to determine existence
            let state = self.state.clone();
            let transaction = spawn_monitored_task!(async move {
                state.get_executed_transaction(digest).await.tap_err(
                    |err| debug!(tx_digest=?digest, "Failed to get transaction: {:?}", err),
                )
            })
            .await
            .map_err(|e| anyhow!(e))??;
            let input_objects = transaction
                .data()
                .inner()
                .intent_message
                .value
                .input_objects()
                .unwrap_or_default();

            // the input is needed for object_changes to retrieve the sender address.
            if opts.require_input() {
                temp_response.transaction = Some(transaction);
            }

            // Fetch effects when `show_events` is true because events relies on effects
            if opts.require_effects() {
                let state = self.state.clone();
                temp_response.effects = Some(
                    spawn_monitored_task!(async move {
                        state.get_executed_effects(digest).tap_err(
                            |err| debug!(tx_digest=?digest, "Failed to get effects: {:?}", err),
                        )
                    })
                    .await
                    .map_err(|e| anyhow!(e))??,
                );
            }

            let state = self.state.clone();
            if let Some((_, seq)) = spawn_monitored_task!(async move{
            state.get_transaction_checkpoint_sequence(&digest)
            .map_err(|e| {
                error!("Failed to retrieve checkpoint sequence for transaction {digest:?} with error: {e:?}");
                anyhow!("{e}")
            })}).await.map_err(|e|anyhow!(e))??
        {
            temp_response.checkpoint_seq = Some(seq);
        }

            if let Some(checkpoint_seq) = &temp_response.checkpoint_seq {
                let state = self.state.clone();
                let checkpoint_seq = *checkpoint_seq;
                let checkpoint = spawn_monitored_task!(async move {
                state
                // safe to unwrap because we have checked `is_some` above
                .get_checkpoint_by_sequence_number(checkpoint_seq)
                .map_err(|e| {
                    error!("Failed to get checkpoint by sequence number: {checkpoint_seq:?} with error: {e:?}");
                    anyhow!("{e}"
                )
                })}).await.map_err(|e|anyhow!(e))??;
                // TODO(chris): we don't need to fetch the whole checkpoint summary
                temp_response.timestamp = checkpoint.as_ref().map(|c| c.timestamp_ms);
            }

            if opts.show_events && temp_response.effects.is_some() {
                // safe to unwrap because we have checked is_some
                if let Some(event_digest) = temp_response.effects.as_ref().unwrap().events_digest()
                {
                    let state = self.state.clone();
                    let event_digest = *event_digest;
                    let events = spawn_monitored_task!(async move{
                    state
                    .get_transaction_events(&event_digest)
                    .map_err(|e|
                        {
                            error!("Failed to call get transaction events for events digest: {event_digest:?} with error {e:?}");
                            Error::from(e)
                        })}).await.map_err(|e|anyhow!(e))??;
                    match to_sui_transaction_events(self, digest, events) {
                        Ok(e) => temp_response.events = Some(e),
                        Err(e) => temp_response.errors.push(e.to_string()),
                    };
                } else {
                    // events field will be Some if and only if `show_events` is true and
                    // there is no error in converting fetching events
                    temp_response.events = Some(SuiTransactionBlockEvents::default());
                }
            }

            let object_cache = ObjectProviderCache::new(self.state.clone());
            if opts.show_balance_changes {
                if let Some(effects) = &temp_response.effects {
                    let balance_changes = get_balance_changes_from_effect(
                        &object_cache,
                        effects,
                        input_objects,
                        None,
                    )
                    .await;

                    if let Ok(balance_changes) = balance_changes {
                        temp_response.balance_changes = Some(balance_changes);
                    } else {
                        temp_response.errors.push(format!(
                            "Cannot retrieve balance changes: {}",
                            balance_changes.unwrap_err()
                        ));
                    }
                }
            }

            if opts.show_object_changes {
                if let (Some(effects), Some(input)) =
                    (&temp_response.effects, &temp_response.transaction)
                {
                    let sender = input.data().intent_message().value.sender();
                    let object_changes = get_object_changes(
                        &object_cache,
                        sender,
                        effects.modified_at_versions(),
                        effects.all_changed_objects(),
                        effects.all_deleted(),
                    )
                    .await;

                    if let Ok(object_changes) = object_changes {
                        temp_response.object_changes = Some(object_changes);
                    } else {
                        temp_response.errors.push(format!(
                            "Cannot retrieve object changes: {}",
                            object_changes.unwrap_err()
                        ));
                    }
                }
            }
            let epoch_store = self.state.load_epoch_store_one_call_per_task();
            convert_to_response(temp_response, &opts, epoch_store.module_cache())
        })
    }

    #[instrument(skip(self))]
    async fn multi_get_transaction_blocks(
        &self,
        digests: Vec<TransactionDigest>,
        opts: Option<SuiTransactionBlockResponseOptions>,
    ) -> RpcResult<Vec<SuiTransactionBlockResponse>> {
        with_tracing!("multi_get_transaction_blocks", async move {
            let cloned_self = self.clone();
            Ok(spawn_monitored_task!(async move {
                cloned_self
                    .multi_get_transaction_blocks_internal(digests, opts)
                    .await
            })
            .await
            .map_err(|e| anyhow!(e))??)
        })
    }

    #[instrument(skip(self))]
    async fn get_events(&self, transaction_digest: TransactionDigest) -> RpcResult<Vec<SuiEvent>> {
        with_tracing!("get_events", async move {
            let state = self.state.clone();
            spawn_monitored_task!(async move{
            let store = state.load_epoch_store_one_call_per_task();
            let effect = state.get_executed_effects(transaction_digest)?;
            let events = if let Some(event_digest) = effect.events_digest() {
            state
                .get_transaction_events(event_digest)
                .map_err(
                    |e| {
                        error!("Failed to get transaction events for event digest {event_digest:?} with error: {e:?}");
                        Error::SuiError(e)
                    })?
                .data
                .into_iter()
                .enumerate()
                .map(|(seq, e)| {
                    SuiEvent::try_from(
                        e,
                        *effect.transaction_digest(),
                        seq as u64,
                        None,
                        store.module_cache(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(Error::SuiError)?
        } else {
            vec![]
        };
        Ok(events)
        }).await.map_err(|e| anyhow!(e))?
        })
    }

    #[instrument(skip(self))]
    async fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<BigInt<u64>> {
        with_tracing!("get_latest_checkpoint_sequence_number", async move {
            Ok(self
                .state
                .get_latest_checkpoint_sequence_number()
                .map_err(|e| {
                    anyhow!("Latest checkpoint sequence number was not found with error :{e}")
                })?
                .into())
        })
    }

    #[instrument(skip(self))]
    async fn get_checkpoint(&self, id: CheckpointId) -> RpcResult<Checkpoint> {
        with_tracing!("get_checkpoint", async move {
            Ok(self.get_checkpoint_internal(id)?)
        })
    }

    #[instrument(skip(self))]
    async fn get_checkpoints(
        &self,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<BigInt<u64>>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> RpcResult<CheckpointPage> {
        with_tracing!("get_checkpoints", async move {
            let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS)?;

            let state = self.state.clone();

            self.metrics.get_checkpoints_limit.report(limit as u64);

            let mut data = spawn_monitored_task!(async move {
                state.get_checkpoints(cursor.map(|s| *s), limit as u64 + 1, descending_order)
            })
            .await
            .map_err(|e| anyhow!(e))??;

            let has_next_page = data.len() > limit;
            data.truncate(limit);

            let next_cursor = if has_next_page {
                data.last().cloned().map(|d| d.sequence_number.into())
            } else {
                None
            };

            self.metrics
                .get_checkpoints_result_size
                .report(data.len() as u64);
            self.metrics
                .get_checkpoints_result_size_total
                .inc_by(data.len() as u64);

            Ok(CheckpointPage {
                data,
                next_cursor,
                has_next_page,
            })
        })
    }

    #[instrument(skip(self))]
    async fn get_checkpoints_deprecated_limit(
        &self,
        cursor: Option<BigInt<u64>>,
        limit: Option<BigInt<u64>>,
        descending_order: bool,
    ) -> RpcResult<CheckpointPage> {
        with_tracing!("get_checkpoints_deprecated_limit", async move {
            self.get_checkpoints(cursor, limit.map(|l| *l as usize), descending_order)
                .await
        })
    }

    #[instrument(skip(self))]
    async fn get_loaded_child_objects(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<SuiLoadedChildObjectsResponse> {
        with_tracing!("get_loaded_child_objects", async move {
            Ok(SuiLoadedChildObjectsResponse {
                loaded_child_objects: match self
                    .state
                    .loaded_child_object_versions(&digest)
                    .map_err(|e| {
                        error!(
                            "Failed to get loaded child objects at {digest:?} with error: {e:?}"
                        );
                        Error::SuiError(e)
                    })? {
                    Some(v) => v
                        .into_iter()
                        .map(|q| SuiLoadedChildObject::new(q.0, q.1))
                        .collect::<Vec<_>>(),
                    None => vec![],
                },
            })
        })
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
) -> RpcResult<SuiTransactionBlockEvents> {
    Ok(SuiTransactionBlockEvents::try_from(
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

fn get_display_fields(
    fullnode_api: &ReadApi,
    original_object: &Object,
    original_layout: &Option<MoveStructLayout>,
) -> RpcResult<DisplayFieldsResponse> {
    let Some((object_type, layout)) = get_object_type_and_struct(original_object, original_layout)? else {
        return Ok(DisplayFieldsResponse { data: None, error: None });
    };
    if let Some(display_object) = get_display_object_by_type(fullnode_api, &object_type)? {
        return get_rendered_fields(display_object.fields, &layout);
    }
    Ok(DisplayFieldsResponse {
        data: None,
        error: None,
    })
}

fn get_display_object_by_type(
    fullnode_api: &ReadApi,
    object_type: &StructTag,
    // TODO: add query version support
) -> RpcResult<Option<DisplayVersionUpdatedEvent>> {
    let mut events = fullnode_api.state.query_events(
        EventFilter::MoveEventType(DisplayVersionUpdatedEvent::type_(object_type)),
        None,
        1,
        true,
    )?;

    // If there's any recent version of Display, give it to the client.
    // TODO: add support for version query.
    if let Some(event) = events.pop() {
        let display: DisplayVersionUpdatedEvent = bcs::from_bytes(&event.bcs[..])
            .map_err(|e| anyhow!("Failed to deserialize 'VersionUpdatedEvent': {e}"))?;

        Ok(Some(display))
    } else {
        Ok(None)
    }
}

fn get_object_type_and_struct(
    o: &Object,
    layout: &Option<MoveStructLayout>,
) -> RpcResult<Option<(StructTag, MoveStruct)>> {
    if let Some(object_type) = o.type_() {
        let move_struct = get_move_struct(o, layout)?;
        Ok(Some((object_type.clone().into(), move_struct)))
    } else {
        Ok(None)
    }
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
    state: &AuthorityState,
    package: ObjectID,
    module_name: String,
) -> RpcResult<NormalizedModule> {
    let normalized = get_move_modules_by_package(state, package).await?;
    Ok(match normalized.get(&module_name) {
        Some(module) => Ok(module.clone()),
        None => Err(anyhow!("No module found with module name {}", module_name)),
    }?)
}

pub async fn get_move_modules_by_package(
    state: &AuthorityState,
    package: ObjectID,
) -> RpcResult<BTreeMap<String, NormalizedModule>> {
    let object_read = state.get_object_read(&package).map_err(|e| {
        warn!("Failed to call get_move_modules_by_package for package: {package:?}");
        anyhow!("{e}")
    })?;

    Ok(match object_read {
        ObjectRead::Exists(_obj_ref, object, _layout) => match object.data {
            Data::Package(p) => {
                // we are on the read path - it's OK to use VERSION_MAX of the supported Move
                // binary format
                normalize_modules(
                    p.serialized_module_map().values(),
                    /* max_binary_format_version */ VERSION_MAX,
                    /* no_extraneous_module_bytes */ false,
                )
                .map_err(|e| {
                    error!("Failed to call get_move_modules_by_package for package: {package:?}");
                    anyhow!("{e}")
                })
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
) -> RpcResult<DisplayFieldsResponse> {
    let sui_move_value: SuiMoveValue = MoveValue::Struct(move_struct.clone()).into();
    if let SuiMoveValue::Struct(move_struct) = sui_move_value {
        let fields =
            fields
                .contents
                .iter()
                .map(|entry| match parse_template(&entry.value, &move_struct) {
                    Ok(value) => Ok((entry.key.clone(), value)),
                    Err(e) => Err(e),
                });
        let (oks, errs): (Vec<_>, Vec<_>) = fields.partition(Result::is_ok);
        let success = oks.into_iter().filter_map(Result::ok).collect();
        let errors: Vec<_> = errs.into_iter().filter_map(Result::err).collect();
        let error_string = errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<String>>()
            .join("; ");
        let error = if !error_string.is_empty() {
            Some(SuiObjectResponseError::DisplayError {
                error: anyhow!("{error_string}").to_string(),
            })
        } else {
            None
        };

        return Ok(DisplayFieldsResponse {
            data: Some(success),
            error,
        });
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
    opts: &SuiTransactionBlockResponseOptions,
    module_cache: &impl GetModule,
) -> RpcResult<SuiTransactionBlockResponse> {
    let mut response = SuiTransactionBlockResponse::new(cache.digest);
    response.errors = cache.errors;

    if opts.show_raw_input && cache.transaction.is_some() {
        let sender_signed_data = cache.transaction.as_ref().unwrap().data();
        let raw_tx = bcs::to_bytes(sender_signed_data)
            .map_err(|e| anyhow!("Failed to serialize raw transaction with error: {}", e))?;
        response.raw_transaction = raw_tx;
    }

    if opts.show_input && cache.transaction.is_some() {
        let tx_block =
            SuiTransactionBlock::try_from(cache.transaction.unwrap().into_message(), module_cache)?;
        response.transaction = Some(tx_block);
    }

    if opts.show_effects && cache.effects.is_some() {
        let effects = cache.effects.unwrap().try_into().map_err(|e| {
            anyhow!(
                "Failed to convert transaction block effects with error: {}",
                e
            )
        })?;
        response.effects = Some(effects);
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
    Ok(response)
}
